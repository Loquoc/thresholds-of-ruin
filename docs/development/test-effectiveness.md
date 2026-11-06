# Test-effectiveness ledger

Keep a small, evidence-backed record of what tests actually catch and what
maintenance they cost. This is not a pass-count leaderboard, a new merge gate or
a requirement to audit every run. Update it at a coherent fix checkpoint when a
useful finding is established. The [testing policy](testing.md) owns coverage,
execution cadence and qualification.

## Counting and classification

- **Production defect:** implementation violates the supported contract; retain
  the intended failing assertion, correction and green regression evidence.
  Say whether it was caught during development or in released/published code.
- **Documentation defect:** a correct test catches wrong documentation. Do not
  call this an outdated test merely because the repair changes prose.
- **Stale/unupdated test:** an approved contract changed but a fixture, mock,
  driver or expected value did not. Establish the intended contract before
  changing the test; never assume a red assertion is stale because code changed.
- **Harness/infrastructure:** setup, build, driver, environment or evidence
  collection prevented/invalidated the intended check. This is not proof of a
  later gameplay failure. A method may have both stale-fixture and harness
  failures; flag that overlap instead of forcing disjoint totals.
- **Unresolved:** insufficient evidence to decide. Keep it visible but outside
  confirmed defect/stale counts until diagnosed. Deliberate new-feature first-red
  work and mutation sensitivity are separate from naturally detected defects.

Count **distinct root causes**, **distinct failing test methods** (fully qualified
owner plus method), and **failed method executions** separately. One root can
break many methods; rerunning a method or running it on another OS/profile adds
executions, not bugs or distinct methods. Multiple assertions/subtests within a
method do not multiply its method count. A suite/job failure is not another test
failure. Record errors, cancellations, waivers and not-run checks separately.
Define the evidence window and deduplicate before adding totals; never extrapolate
a repository-lifetime count from a partial set of logs.

## Text rename confirmation race, 5 November 2026

[Linux release process CI](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42110629349/job/127537320869)
failed `AdventureProcesses.test_session_commands_and_sensory_in_real_process`:
a successful rename followed immediately by `places` still displayed the old
name. The committed acknowledgement can precede its observation, while the text
session request returned at that acknowledgement. This is one production root,
not a scenario-certificate defect or a stale process assertion.

The deterministic first-red regression
`turns::rename_confirmation_waits_for_the_correlated_picture` independently
reproduced the old name after confirmation. Its rejected-request control passed.
This bounded evidence window contains two distinct failing methods and two failed
method executions (one process method, one scripted regression); message-order
subcases are not additional methods.

Successful renames now await a correlated, coalesced Observe completion before
confirmation and the next local command. The fixed client-text integration owner
passed all 204 tests, including both acknowledgement/picture orders and rejected
rename without confirmation or extra observation demand. The original real-process
assertion remains unchanged and requires final-head hosted qualification; no
fresh local server process run or complete platform pass is claimed.

## Save-deferred recovery fixture synchronization, 6 November 2026

[Linux debug CI](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42166863701/job/127725338174)
at `b453330b8` passed 1,344 server-library tests and failed
`runner::stalled_ingress_tests::recovery_stops_when_storage_becomes_overdue_again`
at its post-unlock flush with `StorageFailure`. This is one fixture synchronization
failure, not another demonstrated ingress defect or a loot-label regression.
The test deliberately blocks SQLite but released the lock and requested recovery
without first settling the blocked attempt. That attempt could publish its failure
after the recovery request had cleared the earlier error state.

The fixture now observes the induced failed flush while the lock remains held,
then unlocks and explicitly retries. Both initial setup and recovery use that
ordering. Existing overdue, held-order and accepted-sequence assertions remain;
additional assertions prove the first record remains pending, is durably retried,
and the second record becomes durable without a remaining save error. Production
storage and ingress behavior are unchanged. Fixed-head runtime qualification is
required separately; a source correction alone is not a passing test.

## Loot-label repeated preparation, 5 November 2026

CH-018's deliberate work-count regressions at head `a2bb6830` ran in the
[Linux debug 3D job](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42103641534/job/127513396472).
Compilation, formatting and Clippy passed; the runtime owner reported 635 passed,
two failed and one ignored. The two intended new methods were
`loot_labels::tests::count_only_summary_never_formats_item_names` (15 discarded
name formats instead of zero) and
`loot_labels::tests::multi_pile_update_prepares_scene_once_and_retains_unchanged_nodes`
(three terrain preparations instead of one after positively asserting three
visible labels). The measured scene had six terrain entries, four shaped meshes
and fifteen item stacks: the baseline visited 18 terrain entries and 12 meshes.
This is deliberate optimization first-red evidence, not two gameplay defects or
a measured frame-rate improvement. Platform/profile repeats do not add distinct
methods. The fix prepares selection context once per label update and directly
counts stacks in count-only mode. Runtime camera/animated-occlusion, disclosure,
focus/modal and unchanged-node witnesses accompany the change; fixed-head native
and core CI qualification remains separate from this first-red record.

## Save-deferred ingress retention, 5 November 2026

CH-002 was reproduced against `6fab91097` in the
[Linux debug first-red job](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42098728533/job/127495738606),
using regression head `12cb70421afb`. The bounded evidence window establishes
**one deferred-ingress ownership root, three distinct behavioral failing methods
and three failed behavioral method executions**, all under
`runner::stalled_ingress_tests`:

- `stalled_ingress_is_bounded_and_overflow_only_disconnects_the_sender` retained
  257 requests against the independent 256-request bound.
- `disconnect_reclaims_held_requests_without_reordering_the_survivors` retained
  `gone` between the expected surviving `first` and `last` requests.
- `real_transport_overflow_closes_flooding_stream_but_keeps_queries_available`
  expired its explicit overload/query liveness window with the real writer stalled.

The fourth method, `recovery_preserves_surviving_fifo_and_receipts_after_overload_reconnect`,
failed its positive accepted-receipt setup before exercising overload/recovery.
This is **one separate fixture failure**, not another confirmed production failure.
The fixture now establishes that receipt before locking the writer, avoiding the
SQLite busy-timeout window, and reports the actual setup output if it fails.
Compilation, Clippy, rustdoc and formatting passed. The server library reported
1,331 passes and four failures; later workspace owners were not reached. The
artifact archive's SHA-256 was independently checked. No OOM, total retained-byte
measurement, performance improvement or complete first-red matrix is claimed.

The correction caps deferred admission, reclaims disconnected-stream work and
preserves surviving FIFO and accepted receipts. Follow-up witnesses put an accepted
retry at the overflow boundary, prove a discarded valid request has no receipt,
cover service-side disconnect cleanup and pause a recovery batch when saving
becomes overdue again. Fixed-head platform qualification is separate from the
first-red evidence; all original storage, authorization and transport witnesses
remain required and unchanged.

## Corpse soft-body breathing, 5 November 2026

The shared soft-region solver continued its autonomous breathing drive after an
owning creature died. A bounded first-red window in the
[Linux debug job](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42063334935/job/127370626725)
at regression head `c41bc952` established **one production root, two distinct
failing methods and two failed method executions**. Both
`creatures::soft_tests::owned_soft_breath_stops_at_death_and_returns_after_revival`
and `creatures::soft_tests::composed_leotard_corpse_soft_regions_remain_at_rest_after_settling`
failed at the intended stationary-dead-region assertion, with about 0.002608 model
units of excursion. Compilation, Clippy, formatting and fixture controls passed;
this was not a setup failure. Linux release reproduced the same two failures,
not additional roots or distinct methods. The debug owner reported 633 passes,
two failures and one existing ignored subprocess fixture invoked by its owning
lifecycle test. Later verification stages were not reached in this failed run.

The correction resolves each soft frame's nearest animation owner through its
current hierarchy and suppresses only autonomous breathing while that owner is
dead, including death pre-roll. Passive springs, independent living neighbors,
revival and standalone soft models retain their behavior; no owner entity ID is
stored across asynchronous model transfer. The retained tests cover those
contracts and real nested Leotard composition. Fixed-head runtime and platform
qualification are separate from this first-red result; no native visual approval
or performance gain is claimed. The first-red artifact's recorded merge tree equals the regression
head tree, and its archive digest was verified.

## Event-time sighting absence, 5 November 2026

CH-014 was confirmed in the published baseline `5c87b5719`: event-time notices
measured time since arrival rather than time out of sight. The bounded first-red
window contains **one production root, two distinct failing methods and two
failed method executions**, both in `tor-client-ascii`'s `it` target:
`turns_and_map::event_time_sightings_measure_absence_from_latest_departure` and
`turns_and_map::model_outcome_sightings_use_departure_time_without_refreshing_the_picture`.
Both first encounters passed before an unexpected notice appeared following long
visibility and one tick of absence. The model witness uses real `OutcomeFacts`
and normal application without refreshing the visual snapshot. An earlier
fixture compilation error attempted mutation through an `Arc`; it was corrected
before these runs and is not counted as a production or behavioral failure.

Recording each disclosed departure tick silently fixes the root. All 182 ASCII
integration methods pass locally, including the new boundary, identity, repeated
cycle, reset and silence assertions, and the retained legacy sighting witness.
This is targeted local debug evidence, not native actual-process, Windows,
release or hosted CI qualification. No test was removed or weakened.

## Discovered-corpse idle initialization, 5 November 2026

A focused regression against published main confirmed **one production root,
one distinct failing method and one failed method execution**: `Anim::corpse`
inherited the living constructor's full idle blend. Newly discovered settled
bodies could therefore carry idle/antic motion until that blend decayed. The
`creatures::animation::support_tests::discovered_corpses_start_without_living_fidgets_across_humanoid_outfits`
method failed at its intended pose-equality assertion for Human/Commoner, rather
than a setup or inactive-fixture failure. The run stops at that first witness;
later outfit/body/antic controls were exercised by the green run, not first-red.

The correction initializes only discovered corpses with zero idle blend. Live
fall transitions, their negative pre-roll and ordinary living construction remain
unchanged. The two new regressions pass, covering four humanoid model/outfit
pairs, both body variants, active antic controls, zero-step pose evaluation,
sequential settled poses and ordinary-system revival. Five mapped acceptance
methods also pass: world-space revival/reaction continuity, action exit, held
refresh, repeated refresh and asynchronous loading after death.

The [isolated evidence run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41957003096)
retains the first-red/fixed source, toolchain and binary hashes, check/Clippy logs
and focused results. This is Linux development-profile evidence, not native
visual approval or final-head platform qualification. Local revival bounds do
not replace world-space acceptance. Direct bestiary death assignment and
independent soft-body breathing are outside this constructor fix; no universal
corpse immobility or performance gain is claimed.

## Workflow progress snapshot ordering, 4 November 2026

The CH-015 actual CLI reproduction confirmed one offline-reporting root: selecting
workflow records by attempt alone made equal-attempt evidence depend on input
order. Two core first-red methods in `test_ci_cost_report.CostReport`,
`test_workflow_progress_uses_latest_snapshot_in_either_order` and
`test_actual_cli_progress_order_and_ambiguity_without_writes`, require identical
results and independently expected 60 runner-seconds with a 120-second completed
workflow span. Each failed once before repair in this bounded regression window.
Three additional first-red methods cover equal-time metadata reconciliation,
conflict rejection and invalid chronology/attempt handling. These are explicit
robustness requirements, not three additional discovered production roots; their
subtests do not multiply the five distinct methods/five method executions.

The reconciler now selects the latest aware update timestamp within the highest
attempt, merges compatible equal-time report fields and rejects ambiguity. Job
accounting remains unchanged. The final 27-method accounting suite passes,
including real CLI order reversal and rejection without overwriting outputs,
higher incomplete attempts, timezone equivalence and unchanged input objects.
Independent review also found that the guide's old timestamp-free JSON example
would now error rather than produce its intended lower bound. A new test reads
the literal documented example and reproduced that error before the guide was
corrected; this is one documentation fixture mismatch, not another accounting
production root. The example now explicitly supplies synthetic chronology and
reports the omitted job pages. Verified-empty chronology requirements also have
a rejection witness. This is focused local Python qualification, not hosted CI
or billing evidence.

## Comparison-to-ledger workload identities, 4 November 2026

The bounded first-red run of the new `test_perf_ledger.LedgerFormat` methods
`test_strikes_comparison_can_be_recorded_by_actual_cli`,
`test_active_mobs_comparison_can_be_recorded_by_actual_cli`, and
`test_all_comparison_workloads_have_valid_ledger_identities` exposed **one
production root, three distinct failing methods and three failed method
executions**. The registry method reports two failing subtests, not two methods.
Both actual export processes failed on invalid ledger names before writing;
independent valid-name controls passed. An earlier draft fixture had an unclosed
dictionary and could not import: that setup error is not production evidence.

Two registry literals were corrected without changing benchmark wire labels,
versions or ledger validation. The final affected suites pass: ledger 21,
comparison 31, shared workload reports 37, active-mob reports 8 and strike reports
10 methods. The regressions assert exact identities, timings, counts, source
commit and independently computed raw checksum, and retain duplicate rejection
without writes. These are focused Python results, not application/release or
Windows qualification; no speedup is claimed.

## Projectile fixture cross-client snapshot race, 4 November 2026

PR253's integrated-head Windows debug Python shard failed
`test_projectile_process.ProjectileProcesses.test_text_expertise_throw_launch_payment_recovery_and_source_free_restart`
at its target-readiness assertion before pickup/restart. Captured headless output
shows the observer snapshot at tick 300 with the player not ready, followed by a
target snapshot at tick 344 with the target not ready and observer updates at
344 with the player ready and no intentions. Two connections do not provide an
atomic snapshot pair; the fixture incorrectly treated those observations as one
state. No database, checkpoint trace or gameplay rule change was needed.

`ProjectileSettle.test_player_completion_between_snapshots_does_not_require_target_wait`
reproduces that exact interleaving deterministically and failed at the same
assertion before repair. Across that first-red and the CI failure, this is **one
fixture-synchronization root, two distinct failing methods and two failed method
executions**, not an additional gameplay defect or three aggregate failures.

When the target is not ready, the fixture now refreshes the player once and
returns only if that fresh observation is ready with no intentions. It still
fails if the fresh player is blocked or queued, on target wait errors, or after
16 ordinary target waits without completion. Five deterministic settle witnesses
and the existing launch-wiring witness pass locally. The real process acceptance,
payment, pickup and source-free restart assertions remain unchanged and must pass
in hosted CI; mock schedules do not qualify that boundary.

## Post-merge qualification ZIP fixture, 4 November 2026

[PR #256 CI run 41844918282](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41844918282)
failed `test_ci_postmerge.PostmergeTests.test_malformed_external_shapes_restore_normal_full_without_compiler_setup`
in Linux debug Python shard 2 before its negative mutations: the positive
`fetch_candidate` control rejected an artifact digest mismatch. The fixture
rebuilt ZIP bytes on each mocked API request using the current ZIP timestamp.
An unchanged qualification could therefore have different list/download bytes
across a two-second clock boundary. This is **one harness/fixture root, one
naturally failing method and one observed failed method execution** in this
bounded evidence window, not a production digest-validation or gameplay defect.

The fixture now uses fixed ZIP entry metadata. The new
`test_fixture_archive_digest_is_stable_across_clock_boundaries` first failed on
archive-byte equality with the original helper; that deliberate regression
execution is separate from the natural failure count. It then passed with the
fix, as did all 31 methods in the post-merge qualification test owner. The witness
checks cross-request byte/digest equality, successful production verification
while the mocked clock alternates, and changed bytes when qualification content
changes. Production validation and negative digest/archive tests remain intact.
These are focused local Python results; hosted CI and cross-platform qualification
remain separate requirements.

## Cross-region retreat and save-anchor boundaries, 30 October 2026

The documentation review's mixed flee-distance candidate was reproduced against
unchanged production AI before repair. In
`ai::flee_tests::cornered_flee_does_not_approach_a_threat_across_region_partitions`,
the split six-cell corridor chose `Move(East)` toward a visible threat instead of
`Wait`; its unsplit control and the separate positive rotated-retreat method
passed. This is **one production root, one distinct intended failing method and
one failed execution** in this first-red window. Before that reproduction, a new
fixture's customized species was absent from its Game catalog: one fixture-setup
root caused two failed methods/executions. Those failures are not flee evidence.

The correction uses the same remembered-route step-count policy from actual
post-step frames. Six dedicated methods cover split/unsplit and rotated joins,
one-way escape, missing remembered connections, blocked movement, decision purity
and checkpointed autonomous execution. The affected AI owner passes 76 methods;
the remembered-route unit owner passes 10. These are targeted local results, not
full workspace, platform/release or performance qualification.

Save-owned note-anchor adapters separately preserve current string/null encoding
and validation. The save-schema owner passes 21 methods, including independent
shape/rejection fixtures for snapshots and private optional entry boundaries.
Two journal-boundary tests, eleven history integration tests, two AI execution/
recovery tests and the actual-server action/annotation restart witness also pass.
These characterize compatibility hardening; no prior data-corruption failure is
claimed. Deterministic AI replay compatibility requires its own explicit rules
identity decision, independent of unchanged serialized shapes.


## Uninitialized-navigation retreat regression, 30 October 2026

The first v57 draft made a known current route a prerequisite for every retreat.
Full CI caught a **production policy regression**, not stale fixture expectations:
current visibility and public AI decisions do not require prior navigation
initialization. Four `abilities::movement_tests::autonomous` methods lost their
typed self-step/ordinary-retreat selection; two `abilities::strike_tests` methods
lost atomic escape from a retained paid prefix. Their original assertions remain.

The bounded evidence window is PR209's initial four Linux/Windows debug/release
Rust jobs plus a local reproduction with the retained, hash-verified same-source
executable: **one root, six distinct methods, thirty failed method executions**.
Platform/profile repeats are executions, not additional roots or methods. Standard
server boundaries refresh navigation, so these failures alone do not establish
frequency in an initialized live server. Targeted earlier AI/travel tests missed
the cross-feature public-Game boundary; the validation set now includes the
existing autonomous movement and paid-strike owners.

The repair chooses one metric per decision. Known routes retain route-hop
comparison; absent routes permit a consistent local-coordinate heuristic only in
an already-disclosed unwrapped physical chart within the actor/threat region
and for ordinary same-region steps. Other visible regions do not veto those
local steps; disclosed folds back into the actor’s region do.
Same-region wrapped/rotated portal controls prevent region identity from being
mistaken for an ordinary coordinate chart. No navigation mutation, hidden-link
query or assertion weakening is used. A first correction restored the four movement cases but still rejected the two
paid-prefix cases because their ordinary local retreat coexisted with a visible
second room. That focused attempt adds two failed executions of the same methods
(**thirty-two across both windows**); the region-scoped chart gate
corrects that over-conservative branch. The final focused checkpoint passes
AI 80, travel 10, autonomous movement 5 and both paid-prefix methods, with the
six original assertions untouched. Hosted and local optimized results remain
separate; the initial failed run is not relabeled green.

## Earned-route confirmed-picture recovery, 30 October 2026

PR 210's Windows debug process partition exposed one controller synchronization
root cause in the fixed-seed earned-route method. A causal completion and admission
update retained an older ready picture. The explicit Observe correctly returned
a newer picture with movement recovery still pending, but the route caller
returned it without rechecking settlement. The controller's nonready guard then
failed as intended. The retained frame sequence includes the later ready picture;
this is a driver defect, not evidence of a body-compilation or gameplay defect.
Count one observed failed method and execution, separately from deliberate local
first-red work and later runs.

The caller now settles again against the confirmed picture before returning.
The transport-only regression reproduces the old-ready → confirmed-recovering →
later-ready order and fails before the correction. It checks both returned and
stored state, unchanged command budget, and a single Observe without Wait,
readmission or retry. A terminal-picture control and existing refusal, paid-tail
resume and budget controls remain. Full hosted qualification remains required;
the deterministic caller witness does not measure scheduling frequency.

Local qualification separately retained an interrupted process attempt with no
terminal receipt and one setup error before gameplay: the restored shell omitted
the installed Rust toolchain from `PATH`, so campaign provenance could not run
`rustc --version`. Restoring that environment, without changing assertions or
rebuilding the verified binaries, passed the complete fixed-seed earned-HD,
repeated-execution and source-free-restore method. The setup error is one separate
infrastructure root and execution, not an additional gameplay failure; the
interrupted attempt is neither a pass nor a demonstrated behavioral failure.

## Body-refactor process binary inventory, 30 October 2026

The focused text Polymorph reopen/retry check first failed before gameplay:
its helper also launches a headless observer, but the local binary inventory
included only server, scenario compiler and text client. The missing
`tor-client-headless` produced `FileNotFoundError`. This is one local setup/
inventory root, one failed method and one failed execution, not a gameplay or
persistence defect. Building the omitted executable and rerunning the exact
method passed without weakening assertions or changing runtime source; the
failed receipt remains separate. Select process binaries from helper dependencies,
not only the client named in the method. The owner is
[`MagicItemProcesses.test_text_paid_self_polymorph_reopens_and_retries_without_source_or_private_ledger`](../../scripts/test_magic_items_process.py).

## Projected limb surface investigation, 28 October 2026

Two existing source-contract defects were confirmed while developing the new
sampler. Its deliberate density first-reds and intermediate refinement failures
are development evidence, excluded from these defect counts.

- **Incomplete limb bounds:** the non-contour bounds omitted the belly offset,
  its deeper Z radius and smooth-union growth. The dedicated
  `creatures::geometry::surface_tests::offset_belly_bounds_include_primitive_extents_and_requested_padding`
  failed its independent extent assertion before the fix: one root, one distinct
  method and one failed execution in this dedicated first-red window. The
  ordinary fine-grid closure control passed; an existing published mesh hole is
  therefore not claimed. The coarse-grid matrix exposed the clipping. Bounds
  now include capsule endpoints, the offset ellipsoid and conservative blend
  growth, then padding. Both bounds controls and the complete limb matrix pass.
- **Incomplete sleeve fitting field:**
  `creatures::geometry::surface_tests::projected_arm_recipes_follow_their_shared_source_fields`
  initially failed against the emitted sleeve: one root, one distinct method
  and one initial failed execution. Generation appended underside mass after
  resolving the field used by fitting. That same addition now lives in the
  shared form, preserving its emission order. All 96 arm checks pass, including
  independent physical error and radial silhouette witnesses.

The combined library passed 459 methods with no failures; its one ignored
subprocess fixture is exercised by the passing lifecycle owner. Four native
composition methods passed with a freshly built client. The fixed-cast geometry
reduction does not establish release timing or the requested 60 FPS crowd target.

A subsequent controlled release comparison caught a **development regression**
in the unpublished projected-surface candidate, outside the fixed Rogue cast.
The Mage benchmark's cold mesh payload rose from 22,360,392 bytes on main to
27,734,784 bytes; the earlier tube-only candidate had used 19,770,624 bytes.
Exact-appearance profiling found both folded forearms falling back to a quarter
grid spacing. The dedicated
`creatures::geometry::surface_tests::resolved_mage_forearm_keeps_its_shape_without_excessive_triangles`
first passed independent physical error and radial silhouette controls, then
failed at 59,044 triangles against a 12,000 ceiling. This is one candidate-cost
root, one distinct regression method and one dedicated initial failed execution;
later diagnostic iterations are not additional production defects.
Fine sampling without projection failed the unchanged one-millimeter controls.
A broader matrix found 16 additional over-budget upper-arm recipes: parameter
witnesses of the same candidate-cost issue, not 16 new defects. The correction
resolves upper-arm features on a narrower starting grid and calibrates the local
field estimate to 0.75 mm within the independently tested one-millimeter physical
requirement. All ten surface methods pass, including the 12,000-triangle Mage
guard and 20,000-triangle guards across 96 arm cases. The complete library passes
463 methods with no failures and one intentional subprocess fixture. All four
fresh native composition methods and seventeen focused optimized methods pass.
Fixed-cast profiles cover all 26 Rogue appearances plus the exact benchmark
Mage without a fine-grid fallback. The controlled comparison validated six runs
over three balanced rounds without failures on the same machine. It confirms
the Mage cold-mesh correction (22,360,392 to 18,683,520 bytes, totals over three
cold samples per round). Its cold construction remains slower than main:
p50/p95/max 598.4/605.7/605.7 versus 355.5/358.4/358.4 ms, nine samples per side.
That startup regression remains open; a geometry-budget pass does not close it.
The fresh 101-body release runs at 1800 by 1125 pixels remain below 60 FPS,
with 20.35–20.88 ms median frames and 25.88–26.43 ms p95. Source density and
repeated field evaluations remain shared follow-up work.

The subsequent analytic field-evaluation candidate has five independent math
methods: known primitive values and axis derivatives, numerical slopes, ordered
and selected nested layers, singular/empty-field boundaries, and symmetric
hard-seam fallback slopes. The initial
zero-gradient feature stub passed scalar/finite controls and failed the three
derivative methods as intended; these deliberate feature first-reds are not
additional production defects. Before integration, sampled analytic values
matched the original scalar evaluator bit for bit. Primitive expressions and
field composition now serve both scalar-only and value/gradient queries, with
derivative work excluded from the scalar specialization. The integrated library
passed 467 methods with no failures and one intentional subprocess fixture
before the later Troll profiling correction below. The one-millimeter and
density guards remain unchanged; the bounded-retry checkpoint supplies the
subsequent native and optimized qualification.

Native profiling then exposed one **unpublished candidate geometry regression**
that the synthetic arm matrix had missed. Analytic placement rejected the exact
Troll's right upper arm for an inward face, expanding its source vertices from
4,720 to 75,118 in the fine-grid fallback. The dedicated
`creatures::geometry::surface_tests::resolved_troll_upper_arms_keep_fidelity_and_bounded_density`
passed independent distance and radial silhouette controls, then failed at
150,232 triangles against its initial 20,000 ceiling. This is one candidate
root, one distinct regression method and one initial failed execution in that
window. The correction retries symmetric differences once on the same aligned
grid, preserving the field and its error tolerances. Only a second rejection
uses the fine mesh; failed attempts emit no geometry. The focused Troll check
restores 4,720 vertices, and all 22 SDF and 11 surface methods pass. Its final
12,000-triangle guard also prevents two overlapping coarse attempts from fitting
under the previous ceiling. The generic 96-case 20,000 guard remains unchanged.
The bounded-retry correction passes the complete library: 469 methods, zero
failures and one intentional subprocess fixture exercised by its lifecycle owner.
All four fresh native composition methods and 33 focused optimized SDF/surface
methods pass. Fixed-cast profiling covers all 26 Rogue appearances plus the
benchmark Mage: one symmetric retry (Troll), zero fine-grid fallbacks, and no
density spikes. The cast totals 1,721,261 source vertices and 2,999,532 triangles
across 565 meshes. A fresh 101-body source inventory also found no fine-grid
fallbacks, with six symmetric retries across 1,847 cache-miss parts.

The subsequent controlled model comparison validates six runs over three
balanced rounds. Mage cold p50/p95/max is 482.5/538.6/538.6 ms against
384.0/406.7/406.7 ms on main (nine samples per side); warm is
7.457/10.111/10.111 versus 5.609/6.158/6.158 ms. Its pose median improves, but
p95 rises from 2.700 to 3.419 ms (270 samples per side). Human and Nymph
construction and pose measurements improve; these results do not establish a
uniform speedup. Three clean 101-body runs retain every body in every measured
frame at 1800 by 1125 pixels on the RTX 2060/DX12. They record
47.50/47.43/46.18 FPS, median 20.95/21.08/21.19 ms, p95
23.78/23.52/24.24 ms and maxima 30.18/25.67/210.44 ms. The third run's hitch
remains unresolved. The 60 FPS target remains unmet; these separate fresh-save
operating points are not controlled GPU before/after evidence. Hosted CI and
authorized raw-result publication remain open.

The asynchronous live-loading feature establishes its intended first red at
`creatures::loading::tests::a_cold_request_immediately_shows_the_egg_without_building_the_creature`:
the synchronous stub returns `Biped` instead of the required `Marker` stand-in.
This deliberate feature first red is not an additional production defect.
The loader owns background preparation, bounded work, stale-result rejection,
failure retention and render-reference transfer. Existing scene contracts now
explicitly await readiness in their fixture driver; production updates only poll.
The affected debug checkpoint passes ten loading, twenty scene, six frame
measurement, five capture and seven source-cache methods. The six existing
reaction methods also pass. Loading contracts include immediate eggs, a ready
result installed on the next poll while another figure remains queued, 101
pending requests without blocking updates, stale results, death, failure,
independent deformation assets and complete generated component inventories.
The transfer matrix includes loose hair, robes, cloaks, skinning, serpent
segments and carried lights. Worker collection also drains Bevy's asset change
and handle-drop bookkeeping while preserving cached sources.

Actual-client acceptance passes movement and inventory input while models are
pending, replacement without a subsequent server view, quitting with unfinished
jobs and private-backend cleanup, and 101 fully prepared camera bodies on every
timed frame. The movement run presents 23 initial eggs, moves the player two
cells west while loading continues, then reaches 25 ready models. Capture review
confirms eggs and their generated replacements. The native feature driver first
used an unsupported OS key spelling, then the wrong existing position field;
these are two driver errors in one method over two executions, corrected to the
existing key-tap helper and `world_feet`. Retaining the private backend handle
also makes failure cleanup independent of the client's graceful exit. These
failures do not establish production defects. Optimized checks pass all ten
loading, twenty scene and six frame-measurement methods. Both client profiles
build successfully. The actual optimized client also passes the native movement,
replacement and 101-ready-model sentinels against the unchanged debug backend;
these are client-profile checks, not a full release-server or platform matrix.
Hosted qualification remains open.
During implementation, the death-completion witness preserved death and snuff
controls, then failed because the completed corpse acquired a live hand entry.
Completion now registers a hand only for a current live actor in `Game.actors`.
This is one unpublished candidate defect, one distinct regression method and
one initial failed execution, separate from the deliberate feature first red.
The initial transfer fixture incorrectly read private payloads from the worker
cache after collection, then selected an updo instead of a loose-hair witness;
it now inspects exported payloads and uses the existing loose-hair positive
fixture. The timing fixture's one-second steps exceeded the real stall threshold;
100 ms steps preserve that threshold and reach the ready-scene positive control.
These fixture setup corrections do not establish production defects.

The live startup splash has an intended feature first red at
`startup::tests::splash_stays_until_world_and_visible_render_frames_are_ready`:
the no-splash stub returns `Finish` while the world is unavailable. The production
gate now requires world readiness and render-frame advancement before revealing
the game and finishing the splash. Independent bar-pixel and milestone tests
retain zero, half and completed progress, unchanged title art, the covered
concept controls, sticky errors and monotonic stage updates. The native first
checkpoint passes four methods: presentation before world startup, all milestone
frames and playable entry with eggs, cancellation/owned-process cleanup, existing
movement during loading, and two fresh-save launches that preserve prior saves.
The first startup run enters with 100 creature models still pending; it does not
qualify a frame-rate improvement.

A subsequent actual-CLI regression found one **unpublished candidate startup
defect**: missing connection credentials were checked after splash capture had
already created files. The rejection method first fails at that side effect,
before its diagnostic assertion. Credential lookup now belongs to pure launch
configuration before the splash; world initialization still follows its first
presentation. This is one candidate root, one distinct regression method and one
initial failed execution. The final checkpoint also adds a small/high-DPI display
fit witness for the illustration's layout. The complete debug 3D library passes
487 methods with no failures and one intentional subprocess fixture exercised
by its lifecycle owner. The refreshed six-method native checkpoint passes,
including the credential rejection and existing appearance-override rejection.
All six startup methods also pass optimized, both client profiles build, and
the actual release client passes three native presentation/cancellation/movement
sentinels against the unchanged debug backend. These are local Windows client
checks; hosted platform qualification remains open.

The investigation also confirmed a separate refiner boundary defect:
`sdf::refinement::tests::the_last_allowed_refinement_round_can_complete_the_surface`
failed on an independently defined sphere after the final permitted subdivision.
The loop never checked that final result. It now checks convergence without
adding a fifth subdivision; all six refinement methods pass, including the
pathological work cap. One root, one distinct method and one initial failed
execution in that dedicated window. Outward quad-diagonal repair and shorter
two-edge triangulation have separate topology controls; neither alone resolved
the Mage density regression.

## Rogue head composition, 28 October 2026

This focused local correction confirmed **one production defect in published
main, one distinct Rust regression method and one failing regression execution**.
Review found that head geometry used neck adornments which its cache key omitted.
The new `creatures::rogue_tests::shared_head_cache_matches_fresh_components_in_both_outfit_orders`
failed at the intended cache-history assertion for Commoner to Princess, after
independent fresh meshes established that the choker changes head geometry.
A resolved head component now supplies both generation and cache dependencies.
The regression then passed for Human, Nymph and Phantom in both outfit orders,
and the affected creature generation/animation suite passed.

The new native method
`test_client_process.Native3DProcesses.test_redressed_head_matches_a_cold_render_in_both_outfit_orders`
had **two fixture failures in two executions**, separate from the production
root: construction and refreshed idle poses initially differed, then the narrow
close-up did not meet its positive ornament-pixel witness. Joint-frame comparison
established the first cause; retained captures established the second. The fixture
now explicitly refreshes both poses, asserts identical joint frames and frames
the neck more closely. Neither pixel threshold was relaxed. Native acceptance
then passed with a visible positive witness and identical cached-versus-cold neck
pixels in both directions. These fixture failures are not stale existing tests,
additional production bugs, or platform/profile duplicates. This is local debug
geometry/native evidence, not release-performance or hosted CI qualification.

## Shared tube sampling, 28 October 2026

Review of geometry cost found that a straight 3 cm smoothed tube used 360
triangles because smooth shading unconditionally selected eight path
subdivisions and doubled ring sides.
`props::art_contract_tests::smooth_small_tubes_keep_closed_surfaces_without_blanket_subdivision`
confirmed enclosed outward volume and closed seams, then failed at the intended
density assertion in one regression execution. Adaptive cubic sampling now uses
control-hull and tangent/radius criteria with bounded work, retains authored
knots/radii, and bounds painted spans. Angular sampling honors authored limits.
The tube now uses 80 triangles with the topology and paint controls passing.
Twelve focused tests passed, then the 3D library passed 441 methods with no
failures. Its one ignored subprocess fixture was invoked successfully by
`launch::tests::malformed_startup_reaps_its_owned_server`; it is not a waived
behavioral test. Current Yeti variants contain 31,273–32,977 vertices and
39,380–40,756 triangles, retaining 18 meshes and zero warm mesh allocations.
These are geometry counts, not release timing or GPU performance claims.
All four native composition methods then passed. Matched native captures covered
all 26 monsters; inspected serpent bands, wing outlines, hair curves and fingers
showed no obvious regression. The initially clipped serpent views were reframed
for both binaries before reviewing their complete silhouettes. These views do
not accept the still-unfinished Yeti fur design. Two balanced three-round release
comparisons subsequently validated all twelve runs with no failures on matching
binaries/machine: cold construction and mesh storage improved substantially for
Human and Nymph, while warm spawning and animation were mixed. The first Nymph
pose p95 regression was concentrated in one head round; the repeat had similar
base/head p95 but higher head median/maximum. No animation-speedup or GPU claim
is qualified. Raw bundles remain local pending authorized publication.

## Camera-qualified crowds, 28 October 2026

Two distinct causes were evidenced by the first native run, one failed method
and one failed execution each. They are separate from the deliberate unit
first-red work for the new visibility feature:

- **Reporting/documentation defect:**
  `Native3DProcesses.test_frame_stats_report_the_crowd_benchmark_and_exit`
  confirmed 25 instantiated/posed actors but only 15 with body meshes in the
  camera on all 159 timed frames. Its retained image confirmed right-edge
  clipping. The old claim that the default view drew all 25 was false. Keep the
  instantiated count, add a full-window camera count and use documented distance
  16 for this diagnostic. The same strict native assertion passed with 25 on all
  176 frames after framing was corrected.
- **New fixture error:**
  `Native3DProcesses.test_frame_stats_measure_one_hundred_visible_creatures`
  observed only 97 instantiated and camera-qualified actors, rather than 101.
  The initial spaced square used a Euclidean assumption, while authoritative
  sight is Manhattan range 16. Four corners were undisclosed; other perimeter
  bodies lacked disclosed ground. A spaced diamond reserves sight distance for
  the floor and keeps all 100 NPCs. At distance 34, the unchanged strict native
  assertion passed with 101 throughout 90 frames; the retained image was reviewed.

The five frame-stat unit checks cover per-camera ownership, part deduplication,
off-camera actors, prop/loose-bound exclusions, 101 composed actors, complete
sample-window minima and missing/ambiguous camera data. A separate actual-model
owner confirms generated body meshes for all 26 Rogue recipes. Clippy and
scenario certificate/reference checks pass. Native runs include screenshot
readbacks and establish workload/correctness, not release timing; performance
and hosted qualification remain separate.

## Yeti geometry cost review, 28 October 2026

Maintainer review prompted a per-part debug geometry inventory. The representative
Yeti contained 49,861 vertices and 74,524 triangles; two forearms alone contained
24,032 triangles because detailed skin was still generated beneath the coat
backing. Removing covered forearm, upper-arm, thigh and pelvis cores reduced it
to 36,630 vertices and 48,636 triangles. All six sex/build variants now have
35,893–37,557 vertices and 48,060–49,340 triangles, with 18 meshes and no additional
warm-spawn meshes. These are static geometry counts, not release timing results.
The tighter 40,000-vertex / 50,000-triangle interim guard first failed at the
intended count and then passed; it is refinement first-red work rather than
an additional naturally detected test-defect total. New Yeti bent-pivot witnesses
and the existing centaur witnesses share unchanged angle, ray and minimum-flesh
assertions; all nine selected joint/model/fringe/motion owners passed.
The fresh native Yeti check passed, and its geometry inventory matched the unit
counts; front/rest and side/heavy inspection found no new joint gaps. The
remaining count is still high;
this is a measured reduction, not an accepted final gameplay budget.

## Surface fur development, 28 October 2026

The first surface-coat implementation had **one development geometry defect,
one distinct method and one failed execution**:
`creatures::geometry::fur_parts::tests::surface_coats_embed_roots_and_comb_tips_over_every_side`
found tips outside their required surface envelope. Projecting a tip with its
root normal could push it inside a tightly curved body. Recomputing the normal
at the destination corrected it without relaxing the envelope or flow assertions.
All six fur component tests then passed, along with seven selected existing/new
model, fur-support, profile and walking contracts. The complete-model witness
reports about 48,000–52,000 vertices, 18 meshes and zero warm mesh allocations
across both sexes and all builds. These counts are not timing measurements.
A separate deliberate resize first-red failed at the tip of a half-size patch;
its fixed millimeter lift now scales with the patch. This first-red contract work
is separate from the naturally caught projection defect counted above.
The first new native Yeti method had one fixture-oracle failure: its saturation
filter rejected pale fur (188 pixels against a 200-pixel witness), although the
retained frames showed the model. The corrected oracle compares both row-edge
background colors, requires over 10,000 foreground pixels and rejects an empty
background control above the cast shadow. It changes the measurement rather
than lowering the visibility requirement. This fixture failure does not add a
production geometry root.
The refinement witness
`creatures::rogue_tests::yeti_forearm_coat_keeps_volume_toward_the_resolved_wrist`
first established upper-forearm fur, then failed its distal width assertion. The
inherited fixed-length coat did not span Yeti's longer arm. A shared hanging-limb
constructor now uses resolved attachment reach and preserves fur volume toward
the wrist on both arms, both sexes and all builds. This refinement first-red is
not counted as an additional published defect above.
Preflight also caught a newly extracted test helper borrowing a consumed `Parts`;
its ownership was corrected before linking. That compile diagnostic is separate
from the evidenced behavioral defect and does not increase its method/root count.
The fresh resolved-span Yeti native method passed; three unchanged native
composition methods retain their earlier passing evidence. Visual review still
rejects the close-pile finish as scale-like: green topology/presentation tests
do not establish fur quality. Release/hosted qualification remains pending.

## Warm 3D mesh retention, 28 October 2026

A focused regression confirmed **one production performance defect in published
main, one distinct regression method and one failing execution**.
`creatures::rogue_tests::warm_spawn_does_not_publish_modifications_to_existing_meshes`
first validated its observer with a real mesh mutation, then a warm Nymph
composition emitted 24 modifications to existing meshes. Contact and hair
fitting called mutable asset access solely to reassert an already-set CPU
retention flag. A shared retained-source accessor now changes that flag only
when needed and otherwise reads immutably. The new regression and all 321
affected creature tests passed, including existing private-hair isolation and
posed-contact tests. Clippy passed. This proves removal of spurious asset events,
not a measured speedup; release timings and hosted CI remain pending.

## Shared organic arms, 28 October 2026

During centaur arm composition work, an initial shared root-cap change caused
**one development regression root, two distinct existing methods and two failed
executions**: `art_contract_deltoids_round_off_level_with_the_shoulder_line`
reported a raised crown, and
`geometry::art_contract_tests::art_contract_pauldrons_clear_the_resolved_arm_on_every_build`
reported insufficient plate clearance. A compact horizontal root and reduced
blend overlap restored both constraints without changing their assertions.
Both methods and all 320 creature tests then passed. Three native methods passed: the
new centaur build/motion acceptance, cached-versus-cold head rendering, and
equipment conflict restoration. The centaur tests' deliberate first-red runs
are feature-development evidence, not additional naturally detected defect roots.
This is local debug evidence; release measurements and hosted CI remain pending.

## Seed audit: Polymorph development, 26 October 2026

Reviewed on 27 October, covering the pre-correction library snapshot, local
`full02` Python stage, initial four hosted Rust lanes and the later inspected
Linux-debug Rust repair lane for [PR #198](https://github.com/KCall360/thresholds-of-ruin/pull/198).
The production list below also uses its bounded focused red/green witnesses.
These are **development defects**, not claims that broken behavior reached main.

**Confirmed in this window: 5 production bug roots, 1 documentation defect,
and 49 distinct stale/unupdated test methods.** The stale methods account for
**61 observed failed/error method executions** in the enumerated snapshots (10 + 32 +
16 + 3). That is a lower bound for the development effort, not a lifetime total
or all PR attempts. One stale method also had a harness error. A global stale
root-cause count is not established; maintenance categories below are not bugs.
No production-versus-stale percentage is inferred from this selected sample.

### Genuine defects caught

| ID | Defect and observed failure | Retained regression and correction |
| --- | --- | --- |
| P1 | Paid Polymorph used a provisional outcome after publication finalized the receipt; the visible changed-form case failed with `Rule(InvalidPlan)` after its positive saved-outcome case. | `abilities::polymorph_tests::paid_polymorph_identifies_only_immediate_visible_changed_form` in [Polymorph ability tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/abilities/polymorph_tests.rs). [Execution](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/abilities/execution.rs) now consumes the authoritative committed receipt, retaining the equality guard. Exact regression subsequently passed. |
| P2 | ChooseAgain restoration resurrected a drained Lore purchase instead of restoring an empty choice slot. The ordinary paid purchase/drain/earned-restoration witness reached its intended assertion and found Lore still present. | External red/green witness `OwnedChooseAgainFirstRed.test_owned_choose_again_discards_drained_lore`; permanent `creatures::body_tests::owned_choose_again_preserves_health_and_funding_not_removed_choices` in [body tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/creatures/body_tests.rs). Preserve funded identity, Health and purchase high-water marks, not removed choices. Both ordinary witness and retained regression passed. |
| P3 | Authored restoration refused a lawful sixth retained ordinary attribute receipt despite effective Strength cap 5; valid setup/drain/allocation succeeded, but total HD stayed 24 instead of restoring 25. | External red/green `AuthoredRestoreCapFirstRed.test_retained_authored_sixth_receipt_restores_with_effective_five`; permanent `creatures::body_tests::authored_restoration_reunites_six_lawful_physical_receipts_without_forging_birth` in [body tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/creatures/body_tests.rs). Retain raw ownership and cap its effective contribution. This proves a privileged-builder boundary, not ordinary advancement UI acceptance. Both witnesses subsequently passed. |
| P4 | Strict ChooseAgain decoding accepted extra fields; the malformed restoration payload unexpectedly deserialized. | `creatures::record::body::tests::personal_restoration_records_require_policy_and_exact_stable_birth_owners` in [record tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/simulation/src/creatures/record/body.rs). An empty struct variant replaces the permissive unit variant without changing valid JSON shape; exact focused regression passed. |
| P5 | AdvancementGrowth validation rejected valid owned zero-personal-HD/body-loss states: Wraith owner-page retrieval failed, and the Strength restoration witness received “Advancement is unavailable.” | `session::advancement_tests::canonical_wraith_earned_regrowth_survives_source_free_service_reopen` in [advancement tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/server/src/advancement_session_tests.rs), and `session::magic_item_tests::self_devices::growth_potions::imported_strength_restores_exclusively_then_reaches_combined_cap_independent_of_ring` in [growth tests](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/server/src/growth_potion_session_tests.rs). [DTO validation](https://github.com/KCall360/thresholds-of-ruin/blob/a5b6aaa98f2a893028434d8a1bdefcbf7cd01fba/crates/protocol/src/advancement.rs) accounts for full authored body capacity while retaining strict bounds; both exact regressions and added capacity checks passed. Count this as one root, not two bugs. |
| D1 | The documentation test correctly detected a stale ruleset identity in a historical consumables checkpoint. | `test_documentation.DocumentationTests.test_documents_do_not_mention_other_versions`; corrected the historical prose rather than weakening the current-guide check. The repaired method passed. This is not a stale-test failure. |

P1's red was observed in a diagnostic build with unrelated reviewed production
reversals; it was additional to those intended mutations. Its traced receipt
mismatch, narrow correction and subsequent green support the classification;
the diagnostic is not an unmodified-baseline full run. The ten intentional core
mutation witnesses are not counted as ten additional naturally detected bugs.
Other feature/mutation investigations are outside this seed count; unverified
candidates remain pending rather than being promoted from notes alone.

### Stale-test counts and evidence boundaries

| Audited snapshot | Distinct stale methods | Observed stale-affected failed/error executions | Classification detail |
| --- | ---: | ---: | --- |
| Pre-correction simulation/server library run | 10 | 10 | Of 13 failed methods, 3 exercised P4/P5 (2 roots); the other 10 had stale fixtures/oracles. |
| Local `full02` Python stage | 32 | 32 | 30 of 31 failed methods were stale; D1 accounts for the other. Two of the 3 error methods also had stale setup, including the mixed spawn case below. |
| [Initial hosted run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40407638850), Windows/Linux debug/release Rust | 4 | 16 | Same 4 methods on each of 4 lanes; 3 fixture causes, not 16 bugs. |
| [Repair run, Linux-debug Rust](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40413523424/job/121890062812) | 3 | 3 | 3 later methods/fixture causes. Other repair lanes are not included in this execution count. |
| **Deduplicated total within these snapshots** | **49** | **61** | Different snapshots above contain different stale methods; additional reruns are not silently added. |

Library maintenance categories were: unselected restoration career, obsolete
compiled format, old starter HD total, four synthetic perception fixtures,
missing authoring fields, incomplete wand inventory, and obsolete retained-magic
rejection (10 methods).

For `full02`, categories were: old region format (3), missing supported-body
catalog closure (3), missing copied Chill body module (1), obsolete purchase
metadata (7), old starter HD expectations (15), Vampire prefix length (1), old
Extra Healing sole-HD oracle (1), and incomplete compact mock (1): **32 methods**.
The run itself reported **2,139 passed, 31 failed methods, 3 error methods and
100 waived**, with **33 failure assertions**. These are different units, not
contradictory totals and not a successful local full run.

The compact mock errored because its fixture omitted `hit_dice`. The spawn method
had two stale-catalog subtest assertions and a secondary `FileExistsError`; count
that stale method once and record its harness overlap. The other infrastructure
finding was a benchmark helper's 600-second cold Cargo-build timeout **before
benchmark execution**, not a performance assertion. Thus infrastructure affected
two methods in this stage, one overlapping the stale category; do not sum these
categories into an invented outcome total.

Hosted methods and corrections:

- `rogue_chill::rogue_chill_is_a_purchased_checked_status_signature_with_no_fake_damage`:
  author the unpaid variant's matching immutable package, retaining rejection
  under the original package.
- `rogue_roster::production_flying_roster_executes_authored_basic_melee_primaries_and_damage`
  and `rogue_roster::production_flytrap_uses_ordinary_clutch_tightening_and_single_escape`:
  install each newborn's canonical geometry before owned creature admission.
- `scenario_packages::rogue_projectile_content_is_default_available_and_expertise_is_an_ordinary_choice`:
  buy expertise from personal HD, not locked body HD. These four methods were
  fixed in [the server fixture correction](https://github.com/KCall360/thresholds-of-ruin/commit/a202ca6b12377d9bfb945585c2801701c12d471d).
- `advancement_ledger::saved_current_and_authored_purchases_reject_the_257th_owned_talent`:
  supply explicit nullable purchase identity and validate the positive roundtrip.
- `progression::latest_removal_preserves_retained_health_randomness_and_exposes_zero_hd`:
  expect the bound Health policy while retaining exact source/seed assertions.
- `supply_lifecycle::phase5_container_checkpoint_rejects_positional_creature_record`:
  include the new pending-return field before testing positional rejection.
  These three were fixed in [the simulation fixture correction](https://github.com/KCall360/thresholds-of-ruin/commit/5d1f3d90236cbad8e912f6dbd8e172012511e38f).

Both hosted corrections changed fixtures only and their exact four/three local
regressions passed; they did not justify relaxing production admission or strict
rejection. The later local broad run stopped at 626 passes/zero failures by
maintainer direction, not completion. Four repaired methods passed there;
the remaining 30 failed/error methods plus one affected Service-helper witness
passed in a 31-method selection. Sixteen focused Rust boundary checks also
passed. These scoped results are not an exhaustive local pass; final hosted
qualification remains separate, as recorded in PR #198.

**Evidence limits:** this seed was classified from the original local failure
and green receipts plus the specified hosted logs, not only PR prose. Local
raw logs are intentionally uncommitted and are not presented as reader-accessible
links. The linked PR/CI and immutable merged sources preserve public context,
regression assertions and hosted correction diffs; the local occurrence counts
above are the audit summary, not a claim that those links contain every local
receipt. No lifetime failure rate, complete CI-attempt total, or measured speedup
is established. Remaining unreviewed historical failures are **unclassified /
pending**, not zero.

## Dependency integration: SQLAlchemy 2.1.5

Reviewed 27 October 2026 for [PR #194](https://github.com/KCall360/thresholds-of-ruin/pull/194),
from its dependency-update head `aa70a74a46f819384009313894832c93aaa745fe`
and the bounded local correction receipts. **One production integration defect
root**, not a stale test: the hash-pinned requirements selected SQLAlchemy 2.1.5
while the sampler's runtime `PINNED` allowlist still required 2.1.4. A correctly
installed locked environment was therefore rejected before sampling. This was
caught in development; the evidence does not establish a broken main release.

The existing `test_arena_search_sampling.SearchSamplingTests.test_dependency_allowlist_matches_hash_pinned_requirements`
failed at its independent lock-versus-runtime assertion, then passed after
synchronizing the runtime allowlist without changing or weakening the dependency
hashes. This local window contributes **1 naturally failing method / 1 observed
failed execution / 0 stale-test roots or methods**. Broader hosted errors from
the same mismatch are not independently added to these bounded counts.

Two explicit regressions,
`test_environment_rejects_old_sqlalchemy_before_loading_the_sampler` and
`test_history_rejects_old_sqlalchemy_even_with_valid_integrity_hash` in the same
class, each failed before the correction and passed after it. Those are
**2 deliberate diagnostic red executions**, not additional discovered defects or
stale tests. The retained-history case repairs the integrity hash before testing
semantic environment rejection and also proves the rejected state is unchanged.
All seven selected dependency/history tests passed locally; mocked package
metadata acceptance, exact sampler-source digest, missing-dependency rejection,
syntax and unchanged-lock checks passed separately. Installed Optuna/process and
platform qualification belongs to the exact-head hosted run, not these local
mocked checks.

The follow-through is to update lock pins and their strict runtime allowlist in
one dependency checkpoint, with this cheap independent contract selected before
expensive sampling/process owners. Do not loosen history/environment validation
to make a dependency update pass.

## Rogue content batch: bounded iteration findings

The local catalog checkpoint on 27 October 2026 exposed **1 stale-test root,
2 distinct methods and 2 failed method executions**: after the approved dart and
shuriken identities were installed, two closed catalog-inventory expectations
still omitted them. The intended update script had stopped on a source-anchor
assertion after adding the definitions, before updating those inventories.
`test_rogue_equipment_content.RogueEquipmentContent.test_generated_gear_keeps_source_distributions_and_nonlauncher_damage`
and
`test_rogue_playable_content.RoguePlayableContent.test_selected_character_loadout_declares_equipment_and_finite_known_stock`
therefore failed for the two extra identities. Adding those identities to the
independent expected sets retained all unchanged weapon/armor values, starter
kits and live-pool assertions. Both passed in the subsequent 23-method focused
catalog/fixture checkpoint. This is test-maintenance cost, not a gameplay defect.

The same incomplete-edit run had one setup error in the new treasure example
owner because its deliberately new file was not installed yet; classify that as
incomplete new-feature/harness setup, not another stale root or production bug.
Intentional missing-payload, missing-room-cluster and missing-content first-red
checks are separately retained and excluded from discovered-defect counts.

### Compiler boundary and new acceptance drivers

The real current compiler then exposed **1 production integration defect root**
before publication: the server's authoring validation still rejected every
stackable equipment archetype, even though the runtime's narrow standalone-weapon
contract supported darts and shuriken. One natural compiler invocation rejected
`rogue.dart` in the production catalog. An intentional confirmation using
`test_rogue_throwing_process.RogueThrowingProcesses.test_text_carried_dart_and_shuriken_preserve_exact_pending_payload_after_source_deletion`
failed **1 method once** at that same compiler boundary. That confirmation is not
a second root or an additional naturally discovered test failure.

The repair shares the closed equipment-stack allowlist between authoring and
runtime, retains the outer item-capability exclusions, and adds a full authored
singleton/stack positive with invalid-capability contrasts. It does not change
weapon values or permit stacked armor, launchers or passive modifiers. All **44
current packages**, including the external 3D package, subsequently passed the
real compiler; the complete throwing Text process also passed with source-deleted
reopening and exact hit/miss unit accounting. These are local boundary results,
not an exhaustive or hosted qualification claim.

The first completed pair of new process drivers exposed **2 new-harness roots,
2 distinct methods and 2 failed executions**, not stale existing oracles or
runtime defects:

- The throwing method above attempted a third Focus-cost-2 throw after spending
  its finite pool of 4. The retained response was `Action: Failed.` and no unit
  was released. Ordinary bounded player waits now recover the previewed cost;
  assertions still require the actual admission, one-unit consumption and damage.
- `test_rogue_treasure_process.RogueTreasureProcesses.test_generated_cluster_shares_one_room_uses_next_depth_and_reopens_without_authoring_sources`
  attempted to control a newly born NPC not in the account's initial actor
  allowlist. The fixture now uses an explicit neutral, idle-AI control policy;
  production authorization, catalogs and the selected character remain intact.

Both methods passed after those driver repairs. The throwing method overlaps the
compiler-confirmation method above, so summing category method counts would
inflate a distinct-method total. The genuine native throwing witness remains
**not run locally / required in hosted CI**. Compile/lint preflights and
pre-execution review fixes are not included in these test-failure counts.

### Expired-history fixture premise

The focused server checkpoint reported **373 passed / 1 failed / 0 ignored**.
The failure was
`engine::floor_activation_tests::room_cluster::expired_abandoned_cluster_still_binds_selected_room_area_and_full_footprint`:
its initial assertion assumed floor 2 was absent from the live activation scopes.
Streaming legitimately prepares linked neighbors even with zero configured
radii; the reach frontier can activate floor 2 and prepare floor 3. This is
**1 additional new-harness root / 1 distinct method / 1 failed execution**, not a
production defect or stale existing test. Together with the two process-driver
causes above, this bounded new-harness window has **3 roots / 3 methods / 3
failed executions**; the earlier incomplete-file setup remains separate.

The fixture now targets floor 4, traversing three ordinary descents through
unpopulated earlier floors before rewinding. Distinct start/up/down anchors and
the three-step reach bound keep that target outside the initial frontier. Both
initial and expired-history absent-scope assertions, plus every room-area and
full-footprint corruption rejection, are retained. This is a fixture-only
correction; **its runtime rerun remains pending required exact-head hosted CI**.
A no-link preflight is not a replacement for that witness. The other selected
server owners, including the genuine current save writer/reader and encoded
standalone-stack storage-rejection/retry contract, passed in the same local run.

### First hosted content-batch checkpoint: cosmetic palette oracle

The initial [PR #200 run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40462237905)
at head `ade368b78bfeb0658636ccb9fe8b13db4f5ad8bb` exposed **1 stale-test root**:
current Hallucination expectations still described the 81-object catalog rather
than the 84-object catalog after opt-in dart, fruit and shuriken definitions.
The unchanged presentation adapter deliberately includes named catalog entries,
not only live loot. Independent source-tuple projection and the documented
unsigned selector yield `scroll titled NEM AL` at phase zero and
`scroll titled IV MEM` at phase one; creature samples are unchanged. Neither
the selector nor gameplay behavior needed correction.

This bounded six-job inspection covers Linux debug/release Rust jobs
`122049377146` and `122049377204`, Linux debug/release Python jobs
`122049377227` and `122049377124`, and Windows debug/release Python shard 2 jobs
`122052230753` and `122052230786`: **7 distinct stale methods / 18 failed
executions**, not eighteen bugs. Four Service methods failed in each Linux Rust
lane at their shared exact cosmetic assertion (8 executions); two actual-process
methods failed in each of the four Python lanes (8 executions); the independent
roster-vector method failed in both debug Python lanes (2 executions):

- `session::magic_item_tests::self_devices::hallucination::acquired_map_and_paid_public_plan_keep_historical_cosmetics_after_door_cure_and_source_free_reopen`
- `session::magic_item_tests::self_devices::hallucination::paid_production_dose_selects_exact_visual_cosmetics_without_query_rng_or_authority_changes`
- `session::magic_item_tests::self_devices::hallucination::peer_driven_exact_base_period_publishes_unsolicited_owner_and_followed_visual_updates`
- `session::magic_item_tests::self_devices::hallucination::pending_paid_dose_and_active_palette_reopen_interval_one_without_sources_before_service_pause`
- `test_magic_items_process.MagicItemProcesses.test_imported_hallucination_acquired_map_and_paid_history_survive_door_cure_reopen`
- `test_magic_items_process.MagicItemProcesses.test_imported_hallucination_text_visual_cadence_and_source_free_reopen`
- `test_rogue_presentation_vectors.RoguePresentationVectors.test_complete_authored_rosters_keep_visual_and_monster_sense_channels_distinct`

The repair updates only fixed expected tuples and their palette count across
the current Rust/Python owners, Text assertion, Service adapter, recording
validator and its pure fixtures. Exact identity, privacy, cadence, historical
memory, source-free recovery and negative checks remain intact; expectations
are not computed by the production selector under test. Both actual processes
and ten independent-vector/recording-validator checks passed locally using
unchanged qualified applications (**12 methods total**). Runtime reruns of the
Rust Service owners and hosted partitions remain required; no local relink or
broad rerun is claimed. The still-running Windows Rust lanes are outside this occurrence count.
Historical wire-format examples stay unchanged and valid. The previously
corrected depth-four expired-history owner passed both inspected Linux Rust
lanes; those lanes still failed overall on the cosmetic expectations above.

### Second hosted content-batch checkpoint: private damage publication

The [next PR #200 run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40466465054)
at head `30ec322b0e5c3308a09c2f74e3b3d60ee99d0f5e` passed all 1,213 server
library and 269 server integration methods in both inspected Linux Rust lanes,
including the repaired cosmetic owners. Each lane then reported **2,154 passed /
1 failed** in the simulation library. The failure was
`floor_activation::tests::room_cluster_retains_birth_evidence_after_actor_death_and_rejects_duplicate_ids`.
Its birth, duplicate-identity rejection and death checks succeeded, but final
checkpoint restoration correctly rejected unsealed participant-event receipts
as `RunState`.

This is **1 additional new-harness root / 1 distinct method / 2 failed
executions** in Linux debug/release jobs `122063447022` and `122063447005`, not a
production restore defect or stale existing oracle. The test called private
`apply_damage`, an inner-transaction effect, without completing the outer
publication boundary before checkpointing. The correction follows the existing
gold-population fixture: seal participant events, positively assert receipt
validity, then retain all existing birth/death/identity checks and exact restored
state equality. No production validator is relaxed and no events are discarded.

The repair receives a narrow no-link preflight; **the corrected simulation
method's runtime rerun remains required in exact-head hosted CI**. A source audit
of the other new direct-mutation/checkpoint witnesses is a bounded setup review,
not an additional runtime pass. Other profile executions are outside these two
inspected-job counts.

## Spear and magical-form development, 27 October 2026

This bounded local window covers the first linked affected-contract checkpoint
and its five actual-process checks, before publication. **No production defect
was established.** It found **1 stale-test root / 1 distinct method / 1 failed
execution**, plus **2 new-test harness-premise roots / 2 distinct methods /
2 failed executions**. These categories do not overlap. Deliberate feature
first-red witnesses and source-reviewed corrections made before runtime are
excluded from those counts.

- Stale schema owner: `scenario_package::tests::phase5_container_archetype_rejects_positional_fields`
  still enumerated 23 fields after the approved named `forms` field made 24.
  Add `forms` in declaration order; retain the positive named roundtrip, exact
  field-count equality and rejection of the complete positional representation.
- New ordinary-actor fixture: `abilities::magic_item_tests::physical_devices::ordinary_actor_can_hold_physical_device_without_gaining_creature_only_zap`
  tried to equip into a nonexistent socket: `spawn_actor` intentionally starts
  with empty anatomy. Configure normal rules anatomy without creature statistics;
  retain the equipment, forbidden Zap, charge, hold and checkpoint assertions.
  This setup rejection is not evidence of a physical-device production failure.
- New Adventure fixture: `test_rogue_staves_process.RogueStavesProcesses.test_text_staff_physical_hit_charge_exhaustion_and_pending_source_free_reopen`
  expected `examine wand` to resolve uniquely while carrying both a wand and a
  scroll whose disclosed name includes “wand”. Preserve the ordinary ambiguity
  prompt, choose the disclosed wand through clarification, then keep the exact
  wand/staff noun and unchanged-checkpoint assertions. Earlier physics, charges,
  exhaustion and source-free pending-work assertions passed in the failed method;
  that does not make the whole method a pass.

A separate local evidence-driver failure expected exactly one Rust summary line,
although a passing simulation integration method launches a nested test process.
The integration binary itself passed all 815 registered methods; this was
**1 harness root / 0 failed test methods / 1 aborted evidence-collection stage**,
not another gameplay defect or failed method execution. Preserve its complete
output and distinguish the outer summary when collecting evidence. Remaining
selected simulation-library owners were then run separately rather than repeating
already completed integration targets.

The two Rust fixture corrections passed the no-link server/simulation preflight;
their corrected runtime checks remain required in exact-head hosted CI. The
corrected staff process then passed its exact local rerun, including ordinary
clarification and unchanged simulation state. This is risk-based local evidence,
not a full-suite or hosted qualification claim.

## 3D crowd performance PR, 28 October 2026

Hosted [Windows debug 3D run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40618173786/job/122585477848)
on PR #202 head `9fc7d8a4`: **1 production-defect root / 1 distinct method /
1 failed execution**, caught in published `main` code by a PR run; the PR's own
changes did not cause it.

- `test_client_process.Native3DProcesses.test_mouse_picks_the_presented_item_and_inventory_keys_do_not_exit_early`
  hovered the relic under the player, then on the frame the left button went
  down the hover reported a zombie two cells away. The press selected it, and
  the relic never reached the inventory before the output deadline. Hover
  picking intersected the deliberately oversized culling boxes of deforming hair
  (a 3 m cube), cloaks and robes, so a distant figure's animated box could cross
  the cursor ray. The software renderer's roughly 7 s debug frames only made the
  window easier to land in; local software-adapter runs showed this branch's
  frames are no slower than FXC's.
- Fix: those meshes carry `picking::LooseBounds` and picking ignores them,
  finding the wearer by its body. Intended red:
  `picking::tests::culling_only_bounds_never_capture_the_cursor` failed at its
  loose-box assertion with the positive control passing; it passes with the fix,
  as does `creatures::rogue_tests::every_culling_sized_box_is_marked_loose_and_no_body_box_is`.
- The native mouse methods could not run locally (another window covered the
  client: a harness/environment limit, not a result). Exact-head hosted CI
  remains their qualification.

## Rogue tuning branch, 29 October 2026

Local `feat/rogue-mechanics-profile`, before publication. Window: this
branch's development runs only.

- **Production defect, caught in development (not by a test).** Seeded birth of
  a body-package character rebuilt its authored plan from the birth rows only,
  dropping the growth schedule, so the Rogue adventurer earned XP without
  growing. The seeded-descent campaign exposed it (360 XP at level 1); the
  existing unit and process tests built schedules without seeded birth or only
  checked the starting state. Intended red:
  `creatures::body_tests::seeded_birth_keeps_the_growth_schedule` failed with
  `None` for the next scheduled source; green after the birth path carries the
  unowned future rows. A real-process witness,
  `test_scenario_capabilities_process...test_rogue_adventurer_grows_its_scheduled_row_through_ordinary_play`,
  now drinks Raise Level in the Rogue package. One root, one method, one failed
  execution.
- **Stale/unupdated tests, approved shape change.** Adding scenario capabilities
  and an optional choice quantity left
  `engine::floor_activation_tests::checkpoint_cannot_inject_scenario_policy_into_a_nonpackage_game`,
  `scenario_package::tests::authoring_explicit_actor_values_preserve_canonical_shape`,
  `scenario_package::tests::rogue_magic_catalog_uses_explicit_capabilities_and_appends_finite_stock`
  and `wire::current_wire_protocol_revision_is_explicitly_pinned` expecting the
  prior shapes. The capability and growth-schedule fields were updated in the
  tests; the absent choice quantity is now omitted from authored output instead.
  Two roots (format additions, an absent optional serialized as null), four
  methods, four failed executions.
- **Stale/unupdated tests after merging main, 30 October 2026.** The quick tier
  failed `engine::floor_activation_tests::checkpoint_cannot_inject_scenario_policy_into_a_nonpackage_game`
  (its injected capabilities lacked the new `low_health_flight` switch),
  `runner::backpressure_tests::overdue_saving_holds_commands_in_order_and_nothing_else`
  (a hand-written command kept the pre-merge `expected_revision` field),
  `test_process_module_boundaries` (a process test imported another test module;
  the helper moved to `advancement_package.py`) and
  `test_save_backpressure_process` (its final save assumed durability under full-suite
  load; it now uses the retrying barrier). Four roots, four methods, four failed
  executions. Environment-only failures in the same run (missing `optuna`, a full
  disk, load timeouts) passed or were confirmed environmental when rerun alone.
## Live interface shell checkpoint, 29 October 2026

The new native dock test first failed at the intended missing-HUD assertion
against the previous client. This is deliberate feature first-red evidence,
not a production defect. The first local selected Rust checkpoint passed three
HUD, one pack, four screen, five capture and two map methods. The map owner is
`minimap::cosmetic_tests`; an earlier zero-test `minimap::tests` selection supplies
no coverage.

The first five-method native run exposed two infrastructure roots: Windows'
Default Lock Screen intercepted the new dock and existing world-picking mouse
methods (two methods, two failed executions), and the selected binary directory
lacked the headless client used for cleanup saves (two screen methods, two cleanup
errors). Mouse readback identified the lock-screen window, rather than a UI
action refusal. No mouse assertion was weakened or waived. The asynchronous
movement/egg replacement method passed in that run.

Building the real headless helper corrected the missing executable. A two-method
keyboard retry passed the journal/place/note/history method including its cleanup
save, but advancement timed out before its first game capture. That startup
timeout remains unresolved, rather than being counted as another confirmed UI
defect or a pass. An isolated advancement rerun after the owned compilation
finished passed in 42.825 seconds; the cause of the earlier timeout is still
unknown. The expanded HUD owner then has eight passing methods, covering all six
dock actions, disclosure and bounded globe assets. Together with the twelve
unchanged pack/screen/capture/map methods above, these are twenty selected Rust
methods, not a full library run. These runs do not qualify native dock input,
optimized behavior or hosted CI. The complete interface redesign remains unfinished.

## Inventory browsing and camera framing, 29 October 2026

The independent model's Gear-filter test first returned sword, potion and token
instead of only the sword. This was deliberate feature first-red work; the full
model integration owner then passed 56 methods, including six new browsing
contracts for filtering, Unicode search, stable selection and reachable targets.
The first renderer checkpoint passed 30 selected methods and five genuine native
keyboard methods, covering inventory search/selection, identification and
directional zap, advancement, journal/place/note/history, and movement while
models replace their eggs. These qualify that initial candidate, not later edits.

Visual review caught one **unpublished UI candidate defect**: opening inventory
left the player at screen center under the panel. The native regression
`Native3DProcesses.test_inventory_reframes_the_player_without_moving_the_actor`
first passed its initial centered-camera fixture check, then failed at
`0.5 < 0.4` after opening inventory. This window has one root, one distinct method
and one failed execution. Camera-only framing now uses the actual panel extent;
the regression retains unchanged actor-world and revision assertions. The same
visual checkpoint prompted model-specific item symbols, tiled stone grain and a
hidden empty message panel. Final correction qualification is separate from the
initial candidate's passes. Real mouse input remains subject to the locked-desktop
limitation above; no waiver is inferred and the full interface goal stays open.

The framing/art correction passed 34 selected renderer methods and six native
keyboard methods in debug, plus 13 targeted release methods and three native
methods using the release client with unchanged debug backend helpers. The model's
56-method evidence remains applicable to its unchanged browsing contracts. These
are selected local results, not the full platform/profile matrix or hosted CI.

A subsequent input-routing regression exposed a second unpublished candidate
defect: `minimap::cosmetic_tests::typing_fields_do_not_toggle_the_map_and_the_shortcut_returns_afterwards`
first passed ordinary map toggling, then failed because typing M in inventory
search changed the map from Inherited to Hidden. This is one keyboard-routing
root, one distinct method and one initial failed execution, separate from the
camera defect. The fix captures search and full-screen input before the global
map shortcut; the native inventory witness also checks that typed M preserves
the map state. Its correction qualification is recorded separately.

The routing correction passed all-target debug Clippy, 14 affected unit methods
(three map, six inventory, five capture), and two native debug-client methods:
inventory search/selection including M, and place/note/history typing. The actual
debug client was rebuilt; documentation's 19 methods, Python lint and whitespace
checks passed. Earlier release results cover the unchanged inventory/framing
contracts, not this later map-routing correction. Mouse acceptance and hosted CI
remain unqualified.

## Equipment disclosure and browsing, 29 October 2026

The new presentation-model projection first returned zero sockets where the
fixture disclosed two positions of the same kind. This was intentional feature
first-red work, not a production defect. The projection preserves separate socket
positions and carried-item identities, remembers only disclosed properties, and
compares compatibility by socket identity rather than labels. The model owner
passed 59 methods after implementation.

The initial native equipment method stopped in the Windows driver with
`KeyError: 'F6'`, before delivering the new shortcut. This is one harness root,
one distinct method and one failed execution, not a client behavior failure. The
existing inventory, magic-prompt and asynchronous egg/input methods passed in
that run. Adding the missing F6 virtual-key mapping let the unchanged equipment
method pass in 15.281 seconds: browsing preserved position/revision, Remove
emptied the disclosed socket, Equip restored the same item, and Tab stayed local
to the panel. Later modifier/dice and full-slot presentation witnesses have
separate correction qualification. Mouse and hosted-CI limitations remain open.

The first complete candidate passed four native debug methods, but visual review
showed twelve slot tiles pushing comparison cards below the opening view. The
same equipment native method gained an independent viewport-budget assertion;
its twelve control rectangles were present before it failed at grid bottom
`0.7066666411823697 < 0.57`. This is one unpublished layout root, one distinct
method and one failed execution, separate from the earlier driver error. Compact
slot rows reserve the lower opening view for comparison; drawn empty-slot
indicators also replace the unsupported font symbol seen during review. The
in-progress release pipeline was deliberately stopped before layout changes;
that interrupted run is not qualification and its warm cache was retained.

The corrected layout passed all-target Clippy in debug and release, all 59 model
methods in both profiles, and ten fresh equipment-filtered renderer methods
(four UI and six unchanged outfit methods) in both profiles. Thirty unchanged
debug methods for inventory input, dock actions, pack order, capture, map input
and framing retain their earlier owner evidence, making 40 selected debug
renderer methods. Release additionally passed eight inventory and ten HUD
methods, making 28 selected release renderer methods. Both actual clients were
rebuilt. Four native methods passed in each profile: equipment, inventory search,
magic prompts and asynchronous movement/egg replacement (59.525 seconds debug,
56.528 seconds release). Release-client runs use the unchanged debug backend
helpers, not a full optimized backend matrix.

The corrected capture places the slot-grid bottom at 0.5146666607591841 of the
frame and shows both comparison cards beginning in the opening view. Further
details and actions remain scrollable. A fresh read-only desktop probe still
found Windows Default Lock Screen under the pointer; mouse acceptance stays
unverified and unwaived. Formatting, Python lint, architecture and documentation
checks passed. Hosted CI and the full interface redesign remain unfinished.

## Equipment action priority follow-up, 29 October 2026

The opening-layout root above needed a further correction: item actions still
followed property text and descriptions. The existing renderer method
`equipment::tests::equipment_renders_declared_sockets_unknown_properties_and_original_action_numbers`
first passed its socket, unknown-property and original-number fixtures, then
failed because the selected subtree ordered properties and description before
`1 Remove`. This extends the same opening-layout root to two distinct methods
(the native slot-budget witness and this renderer witness) and two observed
behavioral failures, not another root. An earlier authoring preflight caught a
redundant iterator `copied()` in the new hierarchy walk; no behavioral test ran
in that failed compilation, so it is excluded from those failure counters.

Compact horizontal item headers and actions before properties corrected the
ordering. Native acceptance now checks the actual selected Remove button's
nonzero rectangle lies wholly inside the panel's scroll viewport, alongside the
existing complete slot set, local browsing and real Remove/Equip cycle. The
corrected debug capture places Remove's bottom at 0.6684444546699524 and the
viewport bottom at 0.753777801990509 of frame height; both cards fit in the opening
view for this fixture. Ten equipment-filtered methods and five capture methods
passed, then all four native debug methods passed in 64.027 seconds. The unchanged
59-method model evidence and other unchanged owners retain their prior mappings;
these results are selected local coverage. Optimized correction qualification is
separate. The desktop probe still identified Windows Default Lock Screen, so
mouse acceptance remains unverified and unwaived.

The action-priority correction also passed release all-target Clippy, ten
equipment-filtered and five capture methods, then four native methods in 56.938
seconds using the rebuilt release client and unchanged debug backend helpers.
Both profiles retain the original equipment action/identity assertions and the
new complete-button visibility check. Formatting, Python lint, documentation and
whitespace checks passed. This is selected local UI qualification; it does not
qualify the hosted matrix, unlocked mouse input, a larger character preview or
the remaining interface work.

## Live equipment portrait, 29 October 2026

The feature's camera-boundary regression first passed a one-body world-camera
fixture, then adding a tagged portrait camera made
`frame_stats::tests::portrait_camera_does_not_change_the_exploration_frustum_count`
return None instead of Some(1). This was deliberate feature first-red work, not
a production defect count. Exploration camera readers now exclude the offscreen
portrait while retaining rejection of ambiguous ordinary world views. Camera
tests also preserve actor following and the world camera's cave cutaway with a
second camera present. The portrait shares live meshes and animation; lifecycle
tests cover ready meshes, detached parts, missing entities, actor changes and
original layer restoration without mesh duplication.

The first candidate passed 44 selected renderer methods and five native debug
methods in 77.560 seconds. Visual review still found two **unpublished portrait
candidate defects**: broad hair/cloth deformation envelopes made the figure too
small, and intrinsic image sizing squeezed out the heading. The existing native
equipment method gained two independent subtest witnesses. Its real rendered
figure occupied 0.46022727309013856 of image height, failing the unchanged 0.52
minimum; only 11.000001523643732 pixels remained for the heading, failing the
20-pixel minimum. This is two roots, one distinct method, one failed execution
with two failed subtests; subtests do not inflate method or execution counts.
Existing nonempty-render, identity, viewport and equipment-action assertions
remain intact. Failed authoring preflights for interface visibility/type aliases
and a deprecated handle constructor had no behavioral test executions and are
excluded from those counters.

Camera fitting now excludes loose culling envelopes while still rendering their
meshes; an independent AABB witness verifies both facts. Nonshrinking labels and
an image with zero flex basis reserve caption space. The corrected debug
checkpoint has 45 selected methods: six fresh portrait methods and unchanged
owners for frustum counts (seven), camera (five), capture (five), equipment (ten,
including six outfit methods), inventory (eight) and picking (four). All-target
Clippy and the rebuilt actual client passed. Five native methods then passed in
75.937 seconds, including real portrait pixels/scale/heading, local rotation both
ways, actor reuse, equipment actions, exploration framing, search, magic prompts,
and asynchronous movement/egg replacement. Optimized correction qualification is
separate; the broader interface goal remains unfinished.

The corrected optimized checkpoint passed the same 45 selected renderer methods
and all-target Clippy, rebuilt the actual release client, and passed five native
methods in 75.354 seconds. Native release-client runs use unchanged debug backend
helpers; this is not a full optimized backend or hosted platform matrix. The
retained corrected capture measures a 0.7379518042645412 figure fraction and
50.999998301267624 pixels of heading space. These are UI acceptance observations,
not frame-rate qualification. The unchanged model's 59-method results retain
their earlier source mapping.

The maintainer then explicitly authorized: "Mouse tests are waived." A local
exact-method manifest audits the five mouse-dependent methods on
`Native3DProcesses`: dock opening, filtered inventory/drop, world item picking,
movement/doors/mouse fetch, and selected-actor clicking. The supported direct
runner recorded exactly those five IDs as `waived` in each profile-labelled
selection, with no mouse executions or passes. Other keyboard/rendering and
non-graphical checks remain required, and hosted CI remains unchanged. This
authorization supersedes the earlier unwaived local mouse limitation for this
task; the waiver records are not a clean full-suite gate.

Waiver audit also found one stale oracle by source inspection: the dock test's
expected catalog still omitted the added Equipment action. Its independent
expected set now includes all seven controls, preserving the existing click and
no-world-action assertions. This is one stale-oracle cause and one method, with
zero observed failed executions because the method is waived. The already
captured real frame contains the seven expected controls. An AST comparison
excluding only the five waived method bodies preserves the unchanged context
and bodies of the five already-passing native methods, supporting their reuse
after this isolated oracle correction. The full interface goal remains open.

### Character-sheet navigation fixture authoring (2026-10-29)

The new renderer test
`character::tests::browsing_uses_shared_commands_and_preserves_drafts_and_hidden_screen`
failed on its second independent request: the fixture reset `Game.busy` without
acknowledging the transport's one-request ready gate. A fresh ready test channel
per independent case fixes the fixture while preserving all command and
rejection assertions. The same correction was applied to the new screen test
before it ran. This is one fixture-authoring cause, one observed failing method
and one failed execution, not a production defect. The intentional first-red
for previously absent Page Up/Down navigation is feature work and excluded from
those counts. Compiler authoring errors are also excluded. The fresh character
owner passes two methods, including the independent free-command catalog,
redaction and page-scroll assertions; five screen and ten model-projection
methods pass. Two actual debug-client methods also pass, covering catalog
navigation, typed ledger commands and other full-screen keyboard workflows.
A hash-checked assertion-only delta preserves the production inputs of those
native runs. Release and hosted CI for this slice remain unrun; the explicit
five-method local mouse waiver remains separate from these passing checks.

### Graphical advancement choices and confirmation (2026-10-29)

Five defects were exposed in the new adapter while building graphical purchases:

- `progression::tests::hover_exit_restores_the_card_color_without_erasing_keyboard_selection`
  found that a card stayed highlighted after pointer exit. Feedback now restores
  the base color while retaining a keyboard-selected card's highlight.
- The 3D model integration method
  `advancement::preview_balance_details_are_read_only_and_do_not_offer_hidden_pool_shortcuts`
  found selectable funding choices in read-only preview details. Options and
  dispatch now require an actual Balances or Catalog query, respectively.
- The same integration owner's
  `advancement::supply_display_mode_never_exposes_the_retained_catalog_as_hidden_shortcuts`
  found that a typed supply chooser could be replaced by its retained catalog.
  A typed display-mode flag preserves the shared supply prose and removes hidden
  shortcuts. A renderer visibility assertion and the real-client workflow retain
  the cross-feature witness.
- Visual review found Confirm below the scroll viewport despite a passing native
  keyboard workflow. The owning regression
  `progression::tests::confirmation_stays_outside_the_scrolling_preview_body`
  failed before moving the action into a fixed footer. Native acceptance now
  checks its entire button rectangle against Bevy's actual calculated clip.
- `progression::tests::confirmation_names_the_exact_retained_proposal_while_browsing_other_pages`
  found a generic confirmation label when browsing away from the preview. The
  footer now names the immutable proposal using the shared formatter. Native
  acceptance reads the actual child Text and checks its subject both in the
  preview and while browsing the ledger.

These are five candidate-production causes, five distinct failing methods and
four failed executions (the two model methods failed in one execution). The
intentional first-red for the new freshness API and the initially absent
native graphical choices is feature work and excluded. Profile repeats do not
add causes or failures.

Two fixture causes are separate: the existing
`net::tests::an_aimed_device_takes_the_next_direction_instead_of_a_step` app lacked
its camera input system's Panel resource; adding that resource preserves its
original aiming and movement assertions. The new common method
`advancement::tests::supply_display_mode_is_disclosed_separately_from_the_retained_page`
initially expected the overlay to close on request. It now witnesses the real
boundary: retain the display while pending, then close it on a validated reply.
These are two fixture causes, two methods and two failed executions, not
production defects. Compiler authoring errors and the initial missing
`test-support` preflight feature are not behavioral regression evidence.

The final local risk checkpoint passes 86 selected methods in each profile:
27 shared advancement, seven renderer-independent model, and 52 renderer
(progression nine, character two, screen five, transport eleven, scene twenty,
diagnostics five). Direct pointer-state dispatch through the generated cards
and footer is covered independently of keyboard dispatch. Four native methods
pass with each actual client profile: exact-once ordinary purchase after an
uncommitted preview; preserved typed supplies/ledger, other screens and
confirmation visibility/subject; and movement plus egg replacement during
asynchronous generation. Both native selections use unchanged debug backend
helpers; this is not an optimized-backend or full platform matrix qualification.
The final source is freshly checked after the allocation-free freshness predicate
and explicit confirmation label. An earlier release run was deliberately
cancelled after the clipping defect; incomplete phases do not qualify anything.
The five audited native mouse methods remain explicitly waived locally and
required in CI. Hosted CI and the full UI redesign remain open.

## Character portrait and structured preview composition (2026-10-29)

The new keyboard-only
`Native3DProcesses.test_character_screen_shares_the_live_portrait_and_keeps_command_keys`
initially timed out because its post-release predicate required Home to remain
pressed. The shared `tap` helper already witnesses the pressed frame, then waits
for release. The actual transcript showed the preserved draft and unchanged
portrait angle. This is one new test-oracle defect, one distinct failing method
and one failed execution, not a production defect or a mouse-waiver case. The
corrected predicate checks the preserved draft after release; rotation, revision,
world position, real portrait pixels and pane non-overlap assertions remain.
The focused debug rerun passes. The two intentional first-red feature failures
for absent character portrait activation and result cards are excluded from
production-defect counts.

The selected renderer owners cover portrait activation and shared-rig lifecycle,
command draft precedence, actual-pane camera framing, responsive sheet widths,
structured health/resource/consequence cards, explicit death warnings, supply
page priority, and retained exact confirmation behavior. Visual review confirms
the two panes fit and the confirmation stays outside the scrolling results.
The final local checkpoint passes 83 fresh renderer methods in each profile;
34 unchanged shared/model advancement methods retain their previous debug and
release qualification after input-hash comparison. Seven selected native
behaviors pass with each actual client profile (six original debug passes plus
the corrected character rerun; all seven together in release). These include
movement and egg replacement while models prepare. Both native selections use
unchanged debug backend helpers; this is functional UI qualification, not an
optimized-backend matrix or performance measurement. Clippy, formatting, Ruff,
architecture, 19 documentation checks and whitespace checks pass.
The full interface redesign and hosted CI remain open; the five audited native
mouse methods remain waived locally, never recorded as passes.

## Map/journal composition and bundled-font coverage (2026-10-29)

Visual inspection of the new map/journal found a north-arrow glyph rendered as
an empty box. The static labels used a character absent from the bundled Crimson
Pro font. `map_journal::tests::static_journal_labels_are_supported_by_the_bundled_font`
then failed against the actual rendered label strings and bundled font cmap,
identifying only `↑`. The label now says “NORTH UP”; the regression checks all
static journal labels so a similar unsupported symbol cannot silently recur.
This is one unpublished UI defect, one distinct failing method and one failed
regression execution, originally found by visual review. It is separate from
the two intentional first-red map-opening feature failures. Compiler authoring
errors are excluded from defect counts.

The first debug checkpoint passed 100 selected renderer methods and six actual
keyboard-driven client methods, including enlarged-map pixels, preserved
rename/note behavior, search draft precedence, character portrait, exact purchase
confirmation and asynchronous egg replacement. Visual review also corrected the
place-origin and keyboard-scroll labels before final qualification. An attempted
`interact::` unit filter selected zero methods and is explicitly unqualified;
actual input boundaries are witnessed by the inventory owner and native tests.
The final corrected source passes 101 fresh selected renderer methods in each
profile (map/journal eight, minimap four, HUD ten, screen five, diagnostics five,
character three, inventory nine, portrait seven, progression eleven, camera five,
network fourteen and scene twenty). Thirty-four unchanged shared/model
advancement methods retain their previous debug/release qualification after
comparison of 88 registered input hashes. All six native methods pass again
with each final actual client profile, including the rendered map, preserved
rename/note and draft controls, exact confirmation, portrait, and movement plus
egg replacement during asynchronous generation. Both native selections use
unchanged debug backend helpers; these are functional checks, not a full
optimized-backend/platform matrix or comparative performance measurement.
Final visual review confirms the supported heading, place origins, map aspect
and scrolling help. Clippy, formatting, Ruff, architecture, 19 documentation
checks and whitespace checks pass.
The five audited native mouse methods remain waived locally, not passed; hosted
CI and the full UI redesign remain open.
## Server performance review: bounded local findings, 27 October 2026

This window covers the new active-mob harness, five diagnostic-accounting
regressions, and the subsequent ten-method simulation selection on harness base
`9fe8b6d818cc8c0a3364251e99924237d32b3303` plus the recorded local test changes.
It includes the coherent corrected simulation/server checkpoints described below.
The two initial red selections reused one source-qualified library binary; green
selections reused their separately built binary. Inspected command receipts
report unchanged inputs.
These are local development findings, not a claim that gameplay defects shipped.

### Existing diagnostic contracts

Five deliberately added regressions established **3 pre-existing diagnostic
accounting roots / 5 distinct failing methods / 5 failed method executions**.
Group roots by the missing accounting rule, not by each affected write site:

| Root | Methods and actual red evidence | Correction / qualification |
| --- | --- | --- |
| Shared actor-table mutation accounting omitted insert/replacement and removal. | `actor_store::tests::actor_table_copy_counter_includes_shared_insert_and_replacement` and `actor_store::tests::actor_table_copy_counter_includes_shared_remove_and_edit`: each observed 0 copied actors where the independent two-actor snapshot required 2. | Route table mutations through one copy-accounting seam. Two methods, two failed executions; not four bugs for the subcases. Both methods subsequently passed in the selected local opt-level 0 checkpoint. |
| A weak-only `Arc` identity change was counted as an actor payload clone. | `actor_store::tests::actor_table_copy_counter_excludes_weak_only_disassociation`: observed 1 instead of 0. | Distinguish strong shared ownership from weak disassociation. One method, one failed execution; the corrected method subsequently passed in the selected local opt-level 0 checkpoint. |
| Scene-array clone accounting omitted the sight array on installation and both arrays on a warm read. | `observation::tests::scene_cache_copy_counter_includes_both_arrays_at_install`: 143 instead of 279; `observation::tests::scene_cache_copy_counter_includes_warm_snapshot_arrays`: 0 instead of 279. | Count the arrays actually copied; subsequent shared-ownership optimization must still count any retained public `Vec` copies. Two methods, two failed executions; both corrected methods subsequently passed in the selected local opt-level 0 checkpoint. |

These are instrumented-work contract defects, not five gameplay defects or five
naturally encountered failures. Their explicit diagnostic first-red executions
are retained separately from new performance acceptance below.

### New harness and fixture maintenance

The causal harness selection exposed **3 new-harness roots / 3 distinct failing
methods / 3 failed method executions**:

- `test_active_mob_driver.ActivityEvidence.test_run_preserves_causal_failure_and_records_terminal_cleanup`:
  terminal cleanup evidence was absent, and cleanup could replace the original
  movement failure. Count the shared causal-failure/cleanup publication rule as
  one root; its two subtest failures are one method execution.
- `test_active_mob_performance_report.ActiveMobReportTests.test_wait_only_actor_cannot_hide_behind_active_region_totals`:
  active region totals could conceal an individual AI that only waited.
- `test_active_mob_performance_report.ActiveMobReportTests.test_every_crossing_matches_corridor_geometry`:
  the validator accepted crossing counts inconsistent with the exact corridor
  route. Seven negative subcase failures are one method execution, not seven bugs.

The log's **10 assertion failure blocks** therefore do not mean ten methods or
executions. All three corrected methods subsequently passed in the recorded
63-method focused Python checkpoint (exit 0, unchanged inputs). This is evidence
for the driver/validator contracts, not evidence of meeting a latency target.

One additional **new test-fixture root / 1 method / 1 failed execution** occurred
in `perception::contract_tests::reused_projection_indexes_keep_rotated_portal_sight_distinct_from_stair_disclosure`.
Its abstract stair and hidden-disclosure assertions passed, but its later East
physical portal started at interior x=2 in a width-9 region. The boundary-exit
validator correctly returned `NotBoundaryExit`, surfaced as `InvalidLocation`.
The correction leaves the stair assertions intact and moves the observer to
boundary x=8 before adding the rotated physical portal. No production validator
or privacy assertion was weakened; the corrected fixture passed in the same
147-method simulation checkpoint. This is a new fixture error, not a stale approved oracle
or a production topology defect.

### Intentional performance first-red and preflight boundaries

Six further methods reached their intended operation-count assertions once each:

- `perception::contract_tests::one_observation_builds_each_projection_location_index_once`;
- `observation::tests::warm_observation_reuses_its_geometry_projection_indexes`;
- `awareness::tests::awareness_reconciliation_projects_actor_rows_without_full_observations`;
- `awareness::tests::attack_awareness_capture_and_application_do_not_build_full_observations`;
- `awareness::tests::unchanged_attack_listener_reuses_physical_hearing_geometry`;
- `combat::contact::tests::warm_attack_available_reuses_scene_and_revalidates_contact`.

These **6 methods / 6 intentional red executions** establish new optimization
acceptance, not six additional discovered bugs. In that ten-method selection,
three semantic controls passed and the separate portal fixture above failed.
All six passed after the refactor in the 147-method simulation checkpoint.
These behavioral checks deliberately used a task-local opt-level 0 override with
debug assertions and overflow checks retained. They are not release performance
evidence or default-profile hosted qualification.

Three independent preflight causes prevented later checks, with **0 test methods /
0 failed method executions** each:

1. The new benchmark attempted `LowerHex` formatting on an unsupported digest
   array at three source sites. One compilation root and one failed Cargo check,
   not three bugs. Explicit byte formatting corrected it; the matching later
   example check passed (exit 0, unchanged inputs), as did the baseline release
   build. Those build results are not additional test-method passes.
2. The shared-scene refactor left an unused `adapt::observation` wrapper, rejected
   by `-D warnings`. One development preflight root and one failed Clippy command;
   one slice adapter replaced the wrapper and all callers were updated. The
   combined simulation/server test-target Clippy retry passed.
3. The new fixture used manual `% 2 == 0` parity, rejected by current Clippy.
   Equivalent `is_multiple_of(2)` corrected the example-only lint failure; the
   targeted example check passed. No behavioral test ran in this failed step.


The sustained-activity follow-up has separate reconstructed causality evidence:
two new `test_active_mob_driver.ActivityEvidence` methods reject a hidden middle
gap and missing late/canonical/all-AI coverage. An isolated exact pre-sustained
driver passed the positive boundary fixtures but failed both intended rejection
assertions; the current driver passed the same methods and the complete 18-method
driver owner. This establishes a new harness acceptance-gap correction, not a
game defect or an originally recorded checked-tree first-red. The corresponding
three offline Rust proof tests passed in the selected server library checkpoint.

One executor transport replacement interrupted a local behavioral compilation.
No terminal receipt or test result existed; it is infrastructure-only, not a
failed test method. After checking unchanged source, available Cargo locks and
no compiler/linker process, the exact command retry passed. A separate local
cohort wrapper initially named a nonexistent receipt: zero tests executed; the
corrected invocation reused the verified binary and all 147 selected methods
passed. Neither interruption nor invocation error adds a production defect.

The first simulation feedback wrapper said "Cargo failed", but its underlying
build finished successfully and five tests then failed at the accounting
assertions above. Do not count that wrapper status as an additional compiler root.
Documentation preflight caught two maintenance roots in two methods (one failed
execution each): the new versioned workload was missing from the explicit
workload-identifier registry, and new command examples used development-local
paths forbidden by the guide policy. The registry now names only the actual
workload format; examples use an explicitly chosen output-directory variable.
No version or local-evidence check is disabled. The corrected 19-method documentation
checkpoint passed with unchanged inputs.

Raw local logs remain uncommitted; the bounded counts here summarize inspected
receipts, not repository-lifetime failure rates or exhaustive CI qualification.


### Raw World-fact cache follow-up

At the first-wave head `472d9f680417e9e10f1b4e29523e2c70aa72a310`, one new
operation-count method,
`observation::tests::warm_observation_reuses_raw_world_facts_but_projects_fresh_entities`,
passed its nonempty fixture and fresh-projection controls, then failed because the
second unchanged observation rebuilt raw facts (2 builds versus 1 expected).
This is **1 intentional optimization first-red method / 1 execution**, not an
additional discovered gameplay defect. The bounded raw-fact cache keeps complete
World identity and exact scene/emitter evidence while projecting disclosure,
contact, live door clearance, terminal policy and hallucination fresh.

A preflight caught **1 internal test-helper compatibility root / 0 executed test
methods**: removing the old concealment wrapper missed an independent awareness
witness. A test-only wrapper delegates to the same canonical predicate; the
existing assertion was not changed. The coherent formatting, simulation test-target
and affected benchmark Clippy retry passed with unchanged inputs. No claim of a
production behavior failure follows from that compile-only result.

A separate executor interruption left the first no-link session without a terminal
receipt. Source and Cargo locks were checked before exact-command recovery;
that retry passed. This infrastructure interruption is not a failed test method.

The green checkpoint passed all six new witnesses plus one existing raw-world
hazard method, then 151 remaining affected contracts from the same verified
executable: **158 unique methods, 158 passing executions**. The retained tests
cover portal/body/contact disclosure, fresh hallucination, actor/item privacy and
branch isolation; eviction now also checks release of raw cached payloads. These
checks use the task-local opt-level 0 profile with assertions and overflow checks,
not release timing or default-profile hosted qualification. Server/integration
and real-process final-head qualification remain pending; their earlier passes
are not silently relabeled as coverage of this changed simulation dependency.

The first-wave real headless smoke remains failed: throughput was 0.472 Hz and
source-free restart exceeded its unchanged 30-second limit after successful save
and inspection. A protected-copy diagnostic later restarted in 8.664 seconds;
the earlier failure remains unreproduced and unresolved, not waived. Ordinary
saved-action p95 passed its 8 ms budget, but the larger case exceeded the 33 ms
maximum at both baseline and head. These negative acceptance results are not
counted as discovered implementation roots without a causal diagnosis.

### Exact decision-geometry reuse follow-up

At head `da492247cfb57a6043a2a39e826d983bc57b371a`,
`observation::tests::decision_reuses_witnessed_geometry_but_refreshes_disclosure`
passed its nonempty, identical-view, exact-sight and newly placed item controls,
then failed at the intended work assertion: three scene builds versus one.
This is **1 intentional optimization first-red method / 1 execution**, not a
new discovered gameplay defect. The fix shares the ordinary exact-witness
geometry lookup while projecting current entities, knowledge and effects anew.

The bounded review updated existing operation-count fixtures to explicitly
measure either a cold construction or a warmed lookup. Action, route, tactile
privacy and precommit-plan assertions remain exact. These were proactive
contract updates, not observed stale-test failures; no failed-test count is
invented for them. The geometry invalidation fixture additionally compares
fresh decision sight and observations after chart, body, orientation and death
changes against the uncached computation.

The green checkpoint passed the new regression and 160 affected methods from
the same verified executable: **161 unique methods / 161 passing executions**.
The integration checkpoint passed the warmed finite-supply boundary plus 20
affected public integration methods (21 unique methods). A local selection
preparation typo caused one driver failure before any method executed; correcting
the selection reused the same binary and did not change assertions.

This task-local opt-level 0 evidence preserves assertions and overflow checks;
it is neither release timing nor exhaustive/default-profile hosted qualification.

### Shared cell-array follow-up

The first new copy-count test exposed **1 new-fixture setup root / 1 method /
1 failed execution**, not a production defect: the standard two-room fixture
contains a door and therefore failed its own feature-free positive precondition.
The existing bare-chamber fixture is the correct positive control. No copy-saving
proof was claimed from that setup failure. A prematurely launched green no-link
check was allowed to finish with frozen inputs; original instrumented Vec
production was then restored for the corrected causal red. This avoidable extra
preflight is recorded rather than presented as useful regression evidence.

The corrected positive fixture passed its full-view, fresh actor/item and
stable geometry controls, then failed at the intended copy assertion: 190
`CellView` rows copied versus zero. This is **1 intentional optimization
first-red method / 1 execution**, separate from the setup failure above. The
new regression then passed with shared arrays, independent caller edits and
exactly-once projection detachment accounting.

The passing no-link preflight covers simulation/server test targets and the
benchmark. Its production and consumer input hashes exactly match the green
implementation; the sole subsequent difference is the corrected existing fixture
call in the new test, compiled in the causal red and green runs. That bounded
reuse avoids another identical production lint build; it is not mislabeled as
an exact-tree Clippy pass. The existing public integration tests passed at the
separate geometry-only checkpoint; they are not relabeled as fresh COW evidence.

The final focused library checkpoint passed **162 unique methods / 162 passing
executions** from one verified executable, including live door/trap and
hallucinated-stair detachment with unchanged retained snapshots. The next
integration evidence is the combined release workload and real headless process;
final-main affected integration/server qualification remains required. These
selected local checks are not exhaustive hosted assurance.

## Immutable disclosure and boundary work: local checkpoint, 27 October 2026

The instrumented pre-change checkpoint preserves production behavior at
`45c723dc435d21f0e46b9dbe4d1138f42c8b8e77`. Four new regressions reached their
intended assertions after positive state/privacy/replay controls:

- `control_scrolls::tests::absent_ground_scare_clears_only_eligible_receipts_and_preserves_earned_flee`: eight unnecessary neighborhood probes, expected zero.
- `control_scrolls::tests::unchanged_present_scare_contact_does_not_copy_shared_actors`: two actor payload copies for an unchanged contact transition, expected zero.
- `creature_view::tests::fresh_observations_do_not_copy_owned_personal_inspection_collections`: four owned stat expansions for four fresh observations, expected zero; demanded wire materialization remains positive.
- `engine::observation_reuse_tests::action_projection_reuses_settled_readiness_without_revising_disclosure`: eight observer-local scheduler scans after one settled action, expected zero; full observations, tie ordering and exact replay controls passed.

These are four deliberately introduced performance first-red methods/executions,
not newly discovered independent correctness defects. Two additional new tests
failed setup because their shared fixture supplies starting equipment, contrary
to a new empty-item-store assertion. That is one new-fixture assumption root /
two methods / two failed executions, not stale existing coverage. Their correction
asserts the actual no-ground-scare precondition without deleting equipment or
relaxing state/error assertions. The same absent-feature cause already has the
first-red evidence above, so no redundant old-source link was added.

The first green no-link preflight caught one missed existing consumer migration:
`deep_population_atomicity_tests::assert_original_admission` still accessed
owned `hit_dice` fields directly on the new immutable facts type. Two E0609
diagnostics belong to one stale-test API-migration root / one failed preflight /
zero executed tests. Explicit demand materialization preserves both existing
251-HD assertions. No production behavior was weakened.

The green checkpoint passed **59 unique methods / 59 executions**: 25 simulation
library, 26 server library and 8 integration owners. Both corrected fixture methods
and all four intended-red methods passed. No-link Clippy (simulation/server
libraries, tests and active workload example), formatting and architecture checks
passed. The first architecture invocation had an unset tooling PATH and never ran
Cargo metadata; the correctly configured retry passed. This is a setup error, not
a production or stale-test defect. Release timing, real-process throughput and
final-main/hosted qualification remain pending; no 5 Hz success is claimed.

The documentation check then caught one genuine portability error in this new
review: a citation to ignored, machine-local receipts.
`test_guides_do_not_cite_local_only_evidence` failed once; the report now states
source identities and aggregate evidence without presenting private local paths
as repository references. This is one documentation defect / one method / one
failed execution, not a stale-test failure. The assertion was unchanged.


## 2026-10-27: preparation and witnessed projection components

Three intended first-red performance regressions exposed unnecessary work, not
new gameplay defects: non-due preparation checks made four owned snapshots,
identical streaming geometry made eight body queries, and unchanged full/reference
views repeated two actor classifications. Due-only snapshots and exact existing
scene/visibility witnesses remove that work while preserving dynamic disclosure.
The focused checkpoint covers preparation interruption/overflow/ordered riders,
streaming witnesses, rejected branches, event proofs and server revision privacy.

The initial combined run completed all 61 simulation methods before its server
compile was interrupted without a terminal receipt. The resumed driver first
encountered the abandoned verifier lock; no test ran on that attempt. Free Cargo
locks, unchanged source hashes and the unavailable prior session established the
interruption; the stale lock was preserved and only the remaining owners resumed.
These are infrastructure failures, not production or stale-test roots.
The completed checkpoint passed 78 unique methods: 61 simulation library,
13 server library and 4 simulation integration methods. Clippy preflight passed.
These are focused local checks, not hosted or exhaustive qualification; release
measurement of this batch remains pending.

## 2026-10-28: canonical reach geometry and empty ground work

Two intended first-red performance contracts failed on the unchanged implementation:
`combat::contact::tests::warm_contact_endpoints_reuse_source_geometry_but_resolve_live_target`
resolved 12 body cells rather than four fresh target cells, and
`item_store::tests::empty_ground_index_skips_scene_probes_without_losing_carried_items`
performed eight ground lookups rather than zero. Both positive fixtures passed
before their work-count assertions. One linked simulation artifact qualified both
reds. These are planned optimization regressions, not two newly discovered gameplay
defects or stale-test failures. The cheap green preflight caught one new implementation API mistake: the existing
`RegionMap` has no `is_empty` method (one E0599 diagnostic, zero executed tests).
The fix checks its first indexed entry without adding an index or changing behavior.
This is a compile-time implementation defect, not stale coverage.
Canonical contact geometry remains the only route
algorithm; cold-oracle/live-state and carried/ground mutation witnesses are retained.
The completed focused checkpoint passed 52 unique methods: 35 simulation library,
13 server library and four simulation integration methods. Both intended reds and
the live combat/death/rejected-world witness passed. No-link Clippy passed after
the API correction. These are focused local results, not hosted/full qualification;
release measurement of this batch is pending.


## 2026-10-28: per-actor snapshot ownership

One deliberate first-red performance contract,
`actor_store::tests::shared_actor_edit_copies_only_touched_payload_and_retains_snapshot`,
observed three actor payload copies for one edit in a retained three-actor snapshot,
where the intended bound is one. Positive original/candidate value assertions
passed before that count assertion. This is planned optimization work, not a
new gameplay defect or a stale-test failure. Two existing copy-accounting tests
are deliberately updated to distinguish copied actor payloads from copied shared
map handles; their isolation and owned return-value assertions remain strict.
Weak-only disassociation, unique removal/replacement, indexed mutation, rollback,
visibility and checkpoint witnesses are included in the focused checkpoint.

One preflight invocation repeated `--lib` and Cargo rejected the command before
compilation or tests. This is one driver error, zero test methods and zero stale
fixture failures; the corrected command removes the duplicate argument.
The focused checkpoint passed 29 unique methods: 19 simulation library, six
server library and four simulation integration methods. Clippy preflight passed.
The later release smoke measured 2.115 moves/sec, still below 5/sec; no full or
hosted pass is claimed.

## 2026-10-28: change-driven commit facts

The deliberate first-red
`event_history::tests::commit_event_facts_match_rich_authority_without_materialization`
passed positive identity, receipt, restore and candidate-isolation assertions,
then observed one rich Observation where the intended commit-only path needs
zero. This is a planned performance contract, not a new gameplay defect.

One new fixture/oracle error was caught in
`event_history::tests::commit_event_facts_keep_hidden_identity_and_reject_stale_or_wrong_owner`:
the adjacent invisible source correctly retains tactile Contact authority for
its victim, while the new assertion incorrectly expected no caster identity.
The corrected witness requires exactly Contact (not Creature) and the exact
source ID, while the third-party proof stays empty. Count: one fixture root,
one method, one failed execution; zero stale existing-test failures and zero
production defects. A second new fixture error requested 100 actors from the
small diagnostic constructor, which accepts at most eight; setup failed before
any behavior assertion. The corrected fixture explicitly distributes five
validated positions over each of twenty rooms. This is one additional fixture
root/method/failed execution, still zero stale existing-test failures. The
interrupted server link hit the disk guard; a subsequent retry found its abandoned
verification lock. Both are infrastructure outcomes before tests, not product
failures; obsolete debug executables were removed and the proven-abandoned lock
cleared before retry. The no-link check also caught a new test's private Engine
field access before any test ran; it now uses the existing checkpoint encoder
and simulation restore instead of widening production visibility.

The simulation checkpoint passed 40 event/receipt/boundary methods plus three
reused exact preparation, map-memory and historical-plan witnesses. The new
awareness work-count test passed semantic Game/contact/RNG equality and proved
a real visibility-triggered rescan, then deliberately failed at one unnecessary
actor payload copy versus zero. The source-local guard skips only an identical
contact assignment; it does not skip scans or contests. Its green and the server
integration checkpoint passed. The corrected server first-red then
passed full observation/revision/replay equivalence and failed at exactly 100
rich observations for one local move across the 100-actor fixture, above its
bounded affected-observer budget. The actual server map-adapter fixed-point
witness passed in the same qualified binary.

The wired checkpoint passed all 13 selected server methods and 54 of 55
simulation methods. One existing awareness work-count assertion still expected
two expensive classifications after exact classification sharing had reduced
that to one; its identity/contact/alert/RNG and zero-rich-view assertions passed.
The fixture now explicitly starts its scene cache cold and requires exactly one
classification with an unchanged visibility witness. This is **one stale existing
test root, one method and one failed execution**, not a product defect. Its
focused rerun passed; no unrelated production change was made for it. A no-link
preflight caught private cache-field access in the new cold-cache setup; the
fixture now resets through the existing Default implementation. That compile
error was not an executed test failure. The combined checkpoint qualifies 68
unique methods (13 server, 55 simulation), reusing the 67 earlier passes across
only the test-body/prose correction. Clippy passed on the production changes.
Release measurements subsequently reached 2.937 moves/sec (still below 5/sec);
hosted CI is not claimed.

One additional stale-validator root was caught by all four head-side ordinary
benchmark validations: `validate_work` still required exactly two rich views
per actor, contradicting the intentional boundary-proof fast path. The fix
requires all proof fields together, strict integer counts, exact observer
coverage and bounded fallback/map materialization; absent proof fields retain
the legacy contract. Its positive proof fixture reproduced the intended red,
39 Python tests passed, and all eight retained base/head raw runs revalidated.
Count: one validator root, four failed validations, no product defect. A first
revalidation command incorrectly selected the phase-D case subset; that was a
driver flag error and is retained separately, not counted as a stale test.
No benchmark or Rust build was repeated.

## 2026-10-28: independent input authority and admitted operands

Two deliberate first-red witnesses establish the changed contracts. The wire
fixture first validates its InputContext, then fails because the old Act schema
requires `expected_revision`. The human-operand test admits a natural attack,
equips a different weapon and reaches the intended failure: the queued Default
attack selected that new item. These are explicit semantic changes, not newly
discovered violations of the previous contract. The three causal regressions now pass in focused green qualification. A third causal witness commits a real origin
change and fails because the unconnected runtime registry still holds the old
basis; per-commit publication hooks are now enabled for green validation.

Cheap preflights caught two newly authored InputBasis mistakes (an interior doc
comment and wrapping an already typed ItemId), then passed after correction.
The coherent server preflight caught 23 remaining compile errors in integration
fixtures still using the removed wire field or old decoded shape. These are
migration completeness errors before any affected test executed, not 23 failing
tests or product defects. A later all-target preflight found one additional
map-return fixture with the obsolete wire field; that was corrected along with
an unused fixture actor parameter, and the server preflight passed. A read audit also caught a historical wire fixture from revision 52
being edited instead of the current v64 source; the historical fixture was
restored exactly. The provisional current fixture was subsequently replaced
by genuine final-server process recording; historical fixtures remain unchanged.

The first focused green run exposed two stale assertions after the approved
contract change: `engine::wire_request::tests::durable_wire_retry_resolves_before_a_taken_item_or_revision_is_checked`
still expected `StaleRevision` when resolving an unavailable item as a new
request, although the wire action no longer carries a display revision. Its
same-request durable retry and fresh unavailable-target assertions both passed;
the final expectation is now `InvalidAction`, with no removed safeguards.
`wire::current_wire_protocol_revision_is_explicitly_pinned` still pinned 64
rather than the new 65. Count for this checkpoint: two stale roots, two distinct
methods, two failed executions; both targeted green reruns passed. The other 24
selected server methods, 11 selected simulation methods, and 290 protocol
integration methods passed. These counts do not constitute whole-package or CI
qualification.

The local multi-target runner initially omitted client-common's required
`test-support` feature; compilation failed before any common test executed.
The corrected owner passed all five selected methods, and four ASCII socket
methods passed. The server integration link twice crossed the disk headroom
guard; the owned run was stopped, obsolete archived build artifacts were
removed with manifests preserved, and only the unfinished target was resumed.
Stale wrapper lock refusals performed no additional builds or test executions.
The three selected integration methods then passed. These are harness/resource
interruptions, not product defects or stale test assertions.

The documentation check also found the Hello example and several appearance
handoff references still using the prior wire revision. The example is updated,
while historical handoff prose now describes its original introduction without
presenting an obsolete revision as current. A historical fixture filename in
this ledger was rephrased to avoid the guide's reserved ruleset-name syntax.
This is one documentation maintenance finding, one failing method
(`DocumentationTests.test_documents_do_not_mention_other_versions`), one
execution; not a production defect or a stale test assertion.

Actual-process coverage found one genuine implementation defect in this local
input-authority change: the new private operand/control generations also
participated in spectator readiness numbering. Spectators have no command
permissions, so owner operand/control changes must not become spectator
authority-numbering signals. The input basis already excludes private charges
and reservations; no private-item-value leak was established. The existing regression
`IntentionProcesses.test_readiness_is_authoritative_across_control_changes_and_simulation_execution`
failed at its exact spectator invariant (revision 4 instead of 0); the other 11
selected process methods passed. This is one development production-defect root,
one failing method, one failed execution, not a stale expected value. The fix
excludes spectators from those private generations while retaining controller
ABA protection; the focused real-inventory/control-ABA regression, three related input-context
methods and two existing owner authority/exhaustion regressions pass. The exact
previously failing actual-process case now passes against the rebuilt server,
and the final real wire recording succeeds. The unchanged client/bootstrap
artifacts retain their explicit earlier-build provenance. Pre-fix performance/recording evidence is retained as pre-fix, not
relabeled final-head qualification.

The later frozen-baseline ordinary comparison encountered three additional
executions of the already recorded old boundary-accounting validator failure;
this is the same validator root, not three new defects. Seven complete raw runs
validated under the current oracle without rerunning, and only the final run
interrupted by the disk guard was replaced. The replacement is not temporally
adjacent to its paired head sample. All eight complete runs validate structurally,
but the ordinary maximum-latency budget still fails; validation is not a claim
that the performance target passed.

### Typed dependency evidence checkpoint (2026-10-28)

The first cheap preflight caught two implementation/lint issues: the local
Rust toolchain deprecated `AtomicU64::fetch_update`, and an item before-location
fact was not yet consumed. The checked allocator now uses a compatible CAS loop;
old/new carrier dependencies are explicitly invalidated. A read audit also
removed strong global identity handles that would unnecessarily retain COW
payloads; these were review findings, not behavioral test discoveries.

A new internal region-index invariant test was accidentally inserted twice by
the coordinator and helper before the preflight source snapshot. The compiler
rejected the duplicate name. The exact duplicate was removed, and sole source
application ownership clarified. This is one new-test setup/coordination error,
not a stale existing expectation or a product failure. No broad tier was run.

Four exact causal witnesses failed at their intended assertions before enabling
the preparation/region/observer paths and fixing empty-source bucket lifetime.
All four now pass. The bucket regression is one new implementation invariant
defect discovered in review and confirmed by an internal two-item-row test, not
an observed valid published-game failure. Explicit contributor counts fix it.
The unchanged red binary also passed 28 store/ancestry/weak-root/cold-index
invariants; final selected green passed 159 simulation and 17 server methods.

The first coherent green run passed all four causal tests and 154 other selected
simulation methods. One existing awareness test still required a full perception
rescan after any global visibility change. That is the implementation detail
this batch intentionally removes: remote-disjoint changes can now carry a
certified unchanged classification. The test retains exact Game/contact/RNG and
no-COW assertions, and now requires every eligible observer to be covered by
one rebuilt or certified classification and the newly recorded awareness
boundary to contain all eligible observers. This is one stale-test-maintenance
root, one failing method, one failed execution. It is not counted as a product
bug or a waived failure. The updated test passes, as do all other selected methods.

The selected server runner initially named the spectator test under the generic
module rather than its actual nested owner. One first server test passed before
selector resolution stopped the runner. Resolving the exact registered names
then reused the same verified binary for all 17 selected methods; no rebuild.
This was a selector-driver error, not a test or production failure.

### Simplifying the dependency experiment

The broader typed-dependency cohort passed its focused contracts but did not
establish a matched throughput gain: active-cycle medians were 316.458 ms before
and 317.027 ms after, with 16 samples per side. The real headless run completed
3.029 moves/second, below the unchanged five-per-second goal. Region pin spans
improved, while availability and observer proof work did not demonstrate enough
benefit to justify their complexity. Instrumented scope timings are diagnostic,
not additive estimates or a reason to discard noisy samples.

The next cohort removes positive availability-proof caching and general observer
delta reuse, keeping region contribution ownership and exhaustive empty-residency
classification. Four availability-proof-only tests and two observer-delta-only
tests are removed with those mechanisms, not counted as lost gameplay contracts;
existing canonical availability, target/payment/lifecycle, perception/privacy,
rollback and replay tests remain. The awareness test again uses its original
canonical-rescan assertion because that implementation is restored. The prior
stale-test failure remains recorded above rather than erased from the ledger.
The genuine empty-source bucket lifetime regression remains mandatory.
The simplified cohort passes 153 selected simulation and 17 server methods.
A disk-headroom guard interrupted the server test link before any server test
ran. Existing simulation evidence was retained; obsolete debug CLI executables
were removed with hashes recorded, and only the unfinished server target was
resumed. No release baseline, library cache or source was removed, and this
resource interruption is not a gameplay defect or stale-test failure.


## PR integration: supply-overlay fixture snapshot identity (2026-10-29)

After integrating main's issued-snapshot API, the focused shared advancement
owner reported 29 passing methods and one setup failure in
`advancement::tests::supply_display_mode_is_disclosed_separately_from_the_retained_page`.
Its JSON fixture omitted the newly required `state.snapshot` field. This is one
stale fixture cause, one distinct failing method and one failed execution; it is
not a production defect or behavioral first-red evidence. The fixture now names
the same issued snapshot as its retained page context and independently asserts
that full context before exercising supply-overlay behavior. The existing
retained-page, requested-query and validated-reply assertions stay intact.
No production source changes were needed. The earlier full 3D debug verifier
remains evidence for unchanged production code. The fresh shared advancement owner
passed all 30 methods after the fixture repair, and shared Clippy passed.

## PR release-client check: backend startup deadline (2026-10-29)

The nine-method optimized-client selection passed eight methods. The advancement
method, and one unchanged focused execution of that same method, timed out before
the server published its address; no client window or advancement assertion was
reached. Both executions retain their original 15-second startup deadline and
failure evidence. This is one unresolved startup cause, one distinct method and
two failed executions, not evidence of a graphical advancement defect. The second
trace reached `restore.store.end` at 13.7 seconds, then only reached
`restore.replay.begin` during cleanup at 66 seconds. Two unrelated long campaign
validations were active on the host; that observation does not prove the cause.
The successful full debug gate and eight optimized-client witnesses remain
separate evidence. The remaining boundary is checked with a freshly built,
matching-source optimized backend, without changing its test or deadlines.
The subsequent nine-method selection passed with both client and backend in
release configuration. This qualifies those selected release contracts; it does
not explain the failed debug-backend startup timings or qualify the full matrix.

## Add the next useful finding

One compact row or paragraph per diagnosed cause is enough:

> Date / PR / source state; category and supported contract; exact failing test
> IDs and intended red; fix and green evidence (link when available); distinct
> roots / methods / observed failed executions with window and overlaps;
> unresolved or CI-only coverage. Update an existing cause instead of recounting
> it when another OS, profile or rerun exposes the same failure.

Use the findings to improve fixture update lists, cheap preflights and affected
owner selection. They are not a reason to remove strict assertions, security
coverage, unique boundary witnesses or required native CI.

### Canonical map projection and socket latency checkpoint

Focused development distinguishes intentional causal failures from new test or
runner setup mistakes. Restoring legacy rich-map work, old-side classification,
and TCP batching makes the corresponding new work/socket assertions fail; these
are deliberate first-red witnesses, not newly discovered gameplay defects.

Two setup roots were corrected: the all-target client lint command initially
omitted the existing `test-support` feature, and the new map oracle requested two
units of a non-stackable ordinary item. The latter failed with `InvalidQuantity`
before its intended assertion and is not causal optimization evidence. One
runner initially summarized that setup failure too broadly as an intended red;
its exact assertion evidence was subsequently checked and that one method was
rerun with quantity one. Neither case is counted as a stale existing test or a
production defect; successful independent evidence was preserved.

Two existing observation-reuse methods still assumed map acquisition eagerly
created a rich view after every action. They failed at zero-additional-view count
assertions, while all selected map/disclosure/replay/rollback checks passed.
The revised witnesses count exactly the missing views, require each to materialize
once, and continue requiring repeated reads to reuse the identical boundary.
This is one stale-oracle root, two distinct failing methods and two observed
failed executions, separate from the new fixture setup error above.

### Boundary derivation follow-up accounting

A source audit found that the newly separated intention-derivation duration was
serialized in command profiles but omitted from their exclusive-duration sum.
The ordinary latency report therefore left that duration in its unattributed
bucket. This did not change total latency, game behavior or acceptance budgets;
the retained 123.724 ms tail contained only 0.002283 ms of intention derivation.
The sum and report fields now include each narrow boundary phase, with a focused
unit assertion for exact once-only accounting. This is an audit-found diagnostic
defect, not a defect credited to a previously failing test. Existing timing
receipts remain unchanged; no new stale-test failure is inferred from this audit.

### Borrowed growth-retry ownership follow-up

The new shared/unique allocation-identity witness passed and demonstrated the
old mutable no-op detachment versus retained ownership after a borrowed retry
decision. Thirteen other selected simulation methods also passed. This is causal
refactor evidence, not a claimed historical first-red production defect.

One newly authored pending-recovery-context method failed its positive fixture:
the creature had no contextual recovery source, so canonical normalization
correctly retained `Known { sensed_hostile: false }` instead of the test's expected
`true`. The corrected fixture provides and asserts that capability and the actual
recovery period before exercising context success and error ordering; its
focused rerun passed. This is one new-fixture root, one distinct
method and one failed execution, not a stale test following an approved contract
change and not a production defect. Successful independent evidence is retained;
only the corrected method and unfinished targets need new execution.

A disk guard later interrupted only the unfinished server test compilation.
All successful simulation evidence was preserved. Inactive duplicate artifacts
and obsolete library variants were cleaned with recorded hashes; the server-only
resume passed all six selected methods. This is an infrastructure interruption,
not an additional failed test method or a production/stale-test defect.


### Causal/display separation: preflight accounting

The protocol66/archive64 migration deliberately changes display revision ownership,
snapshot identities and event delivery. Static preflight found unupdated test and
benchmark constructors (required snapshot fields, formerly-Copy bases, private
causal guards and removed revision-map internals). These are compile-time fixture
maintenance, not observed failed gameplay methods. The initial package-selection
error was a driver/workspace setup issue; helper visibility, a stale annotation
argument and one Clippy signature warning were development integration errors.
No stale-method execution count is inferred from these compiler diagnostics.

Source review also identified two semantic development defects before execution:
legacy tick/value narration gates could duplicate old receipts or omit identical
same-tick new receipts, and post-commit outcome projection failure could leave a
live stream without its committed event. Causal batch/suffix capture and explicit
stream disconnection address those respectively. Focused tests retain legacy
counterexamples and exact expected outcomes; these are audit-found issues, not
credited as historical failures caught by an already-running test suite.
The 68 recorder-driver unit methods passed; this does not qualify Rust behavior
or the performance goal.

The first focused Rust pass passed both protocol methods and 27 common-client
methods. Three newly authored outcome-stream methods failed before their intended
assertions because their positive fixture read the not-yet-recorded wire66 golden.
The fixture is now independently constructed and validated, removing that circular
qualification dependency. This is one new-fixture/setup root, three distinct
methods and three failed executions, not three gameplay defects or stale existing
tests. The 29 successful methods are preserved; only these three and unfinished
targets are selected for the resume.

### Causal/display separation: ordered text exchange resume

The focused green-resume pass had **one failing method execution**:
`turns::an_exchange_of_blows_is_told_in_order_and_a_death_ends_it`.
Its actual passage stopped at preparation instead of the expected interrupted
exchange and HP loss. Diagnosis established two overlapping roots, not two
executions:

- **One stale existing-script root / one distinct method:** the script sent an
  Immediate Ack for admitted Act work and changed only visual `ready`, leaving
  authoritative Readiness open. Settlement correctly trusted that purported
  completed immediate request. The script now sends Admitted, Started, closed
  Readiness, its unchanged ordered outcomes, Resolved, then reopened Readiness.
  No expected prose or production permission checks were weakened.
- **One development production narration regression:** `Chronicler::outcome`
  ignored the disclosed `OwnAction.basic_attack` flag, rendering generic ability
  preparation instead of the existing basic-attack phrase. It now uses that
  flag, not an ability ID/name heuristic. The new focused method
  `engine::chronicle::tests::outcome_preparation_prose_uses_disclosed_basic_flag_not_ability_name`
  compares new and legacy beats for both flag values with a renamed ability.

Both issues were exposed by the same original execution. Fixes are staged;
no green rerun is claimed here, and the newly added witness has not yet run.

The new basic-flag unit witness subsequently **passed** and is preserved without
rerunning. The existing exchange method had a **second failed execution**, this
time while validating its reconstructed queued stream before any prose assertion.
The first fixture correction had omitted the Queued intention update: an Admitted
receipt does not populate `ClientState.intentions`, so a first-seen Started update
correctly failed `InconsistentState`. The script now emits Queued before Started,
with the same exact intention/entry identity. This is an incomplete correction of
the same stale-script root (one distinct existing method, now two failed
executions), not an additional production defect or another failure of the
successful basic-flag witness. No further rerun result is claimed here.

The first focused server pass preserved 22 successful methods and exposed two
new/migrated test-setup errors (two methods, two failed executions): the new
frozen-creature inspection fixture used a bare two-room actor without inspectable
creature state, and the migrated broadcast ownership assertion compared a retained
server Arc to an independently decoded wire Arc. The inspection witness now uses
the existing authored mob-arena fixture and still requires actual live damage;
the ownership witness retains the original server Arc and checks both pointer
stability and equality with the received picture. Neither correction changes
production behavior or weakens frozen-picture semantics. Reruns pending.

The corrected frozen-inspection and broadcast-ownership witnesses and original
text exchange subsequently passed; 69 unique focused Rust methods were green
before pre-PR cleanup. An environment interruption killed one remainder compile;
its orphaned verify lock caused a setup-only retry failure and was preserved and
cleared after confirming the owned tool session was gone. Neither is a failed
behavioral method.

Independent review then found a genuine pending-pause ownership regression:
fallible observer capture after `mem::take` could lose the current and unvisited
actors' required suspensions. The fix retains each obligation until suspension
and pause succeed, with a two-actor capture-failure/retry witness. This is an
audit-found defect, not credited as an already-executed test catch. The same
review found Observe flush could precede live reads, foreign outcome self-target
was not checked, and live client state could masquerade as a reset Snapshot.
Targeted causal fault witnesses accompany that coherent cleanup; no green rerun
is claimed until its focused receipt completes.

The first cleanup behavioral pass preserved 34 successful common-client methods,
including blocked-flush ordered delivery and foreign-self rejection. The newly
authored sequence-cancellation witness failed setup validation because it set
active preparation without its required remaining duration. Its positive fixture
now supplies that duration and validates OutcomeFacts before reduction. This is
one new-fixture method/one failed execution, not a production behavior defect.

The cleanup resume passed the corrected sequence fixture, seven text methods,
and 19 of 21 selected server methods. Two remaining setup/expectation failures
were not gameplay defects: a migrated cold test indexed a view cache that is now
legitimately lazy, and a new round-trip test expected Wizard shorthand spelling
instead of its established canonical JSON encoding. The former still requires
no stale disclosed plan in any retained cache and exact final knowledge/privacy;
the latter checks requested display basis and exact decoded saved operands.
These are two failed methods/executions, recorded separately from production bugs.

The two remaining cleanup expectation corrections passed. Across preserved
cleanup receipts, all 63 selected methods pass. The 69 earlier phase21 successes
remain separate evidence; overlapping methods are not double-counted as new
coverage. No release or performance qualification is implied by these checks.

### Demand-driven picture harness and compact semantic follow-up

The first real headless run exposed one stale driver contract: it treated a new
ready notification as evidence that the retained picture had also refreshed.
With separate outcomes and pictures, this compared the old center to the
completed move's destination. Count one stale driver root and one failed
actual-process execution, not a failed movement. The corrected driver waits for
both next readiness and a newer picture, then retains the exact destination
assertion. A fresh but wrong destination still fails. Restart equality compares
all disclosed contents and branch/history while excluding only attachment-local
picture identity and generation. Twenty pure driver tests passed; the second
actual-process run completed with save/restart equality. Its measured throughput
still missed the target and is not presented as a test success for performance.

The compact semantic-input follow-up passed four protocol methods and thirteen
server methods. Its independent review found an ownership regression before
release: canonical visual fallback was repeated per recipient rather than
shared per observing actor. This was audit-found, not credited as a test-caught
bug. The correction uses one transient per-actor picture pair; the existing
one/eight/thirty-two-watcher witness now checks zero adaptations for unchanged
pictures and exactly two for changed pictures, regardless of watcher count.
The corrected thirteen server methods and scoped Clippy passed. The protocol
methods were unaffected and reused, not rerun or counted twice.

One selector-registration error stopped a remainder dispatcher before running
methods; it was corrected from the built binary's actual test listing and reused
that binary. An interrupted executor and a later disk guard interrupted compiler
owners, not gameplay tests. Their incomplete receipts were retained; completed
checks were reused. Exact obsolete artifacts were reclaimed, with protected
binaries and hashes retained, before resuming only the unfinished target. These
are harness/infrastructure incidents, not additional stale gameplay test methods.

The documentation gate caught one genuine documentation defect in this report:
a guide cited a local-only receipt path. The published prose now contains the
measurements and limitations directly, rather than a nonportable evidence link.
This was one failed documentation method/execution, not a stale test failure.

### Current-main integration of causal presentation

The integrated no-link preflight caught one stale test assertion introduced by
combining the staff feature with the input-authority migration: the text staff
command test still constructed `Command::Act.expected_revision`, a removed field.
The text parser now returns the unchanged exact item/aim action; the connection
owns input authority. Removing only that obsolete expected field preserves all
staff capability, charge-independence and aim assertions. This is one stale-test
root/method and one failed preflight, not a production staff defect.

The recorder audit also exposed two remaining stale harness contracts from
separating causal outcomes and pictures. `act` could return a new lifecycle or
readiness frame with the old picture; a focused fake-stream first-red failed
at the intended old-versus-new picture assertions (two methods, five subtest
assertions). The harness now tracks the actual ordered visual-dirty signal and
finishes only already-demanded picture work; causal-only outcomes do not wait
for a picture or issue reset requests. Lifecycle and paid-pause selection remain
unchanged. Positive tests cover picture-before-lifecycle and no-queued replies.
Cross-attachment discovery/amulet comparisons now exclude only picture identity
and generation; all disclosed mechanics stay exact, with a negative comparator
witness for changed tick/health. These are stale harness expectations, not new
production movement or objective defects; actual recorder qualification follows.

The first integrated wire recording captured a real fragmented snapshot but
failed its required explicit-save cleanup under an artificial 512-byte frame
budget. Current contextual control replies can exceed that old budget (captured
admission envelopes were 554–557 bytes); deliberately undersized non-snapshot
frames disconnect by contract. Fresh and resumed-save probes both failed at512
and passed at1024. The recorder now uses1024, still requires an actual snapshot
part and asserts its encoded size stays within that limit. Production budgets,
backpressure behavior and cleanup requirements are unchanged. This is one stale
fixture-budget root and one failed recorder execution, with the separate bounded
probes retained—not evidence of a successfully recorded fixture yet.

The next recorder pass reached advancement and exposed one more caller of the
same stale-picture completion assumption: it explicitly waited for queued work
to leave admission, then immediately issued picture-bound inspection. It now
finishes already-demanded picture delivery before that inspection; the original
active-respec assertion and query remain unchanged. This is one additional stale
caller/method execution under the existing causal/picture harness root.

Recording then reached a Text device case whose pickup assertion expected the
old debug event label `Taken`, although committed outcomes now render canonical
prose (`You pick up ...`). The downstream exact inventory, tick, charge and RNG
assertions remain. A source audit batched the same obsolete pickup/wait labels
in affected process tests rather than discovering each through a separate run.
One actual recorder execution failed at this stale Text-label root; source-only
corrections elsewhere are not counted as additional test failures.

The recorder source audit found anonymous utility-event selectors still reading
visual deltas rather than committed Outcome combat facts. The focused pure
first-red had five intended assertion failures across ten selected methods;
the corrected selection passed all ten. Capture now retains the actual ordered
Outcome; identity, target, applied-result and hidden-ability rejection assertions
are unchanged, and visual payloads are explicitly rejected as event sources.
No production wire sample was synthesized. This is a stale event-source fixture
root detected before reaching that branch of the actual recorder.

The fourth recorder run passed earlier cases and reached growth diagnostics.
Its second inspection reused the wizard's old frozen picture: private growth
credit need not dirty that picture, so the report correctly lacked the newer
record. New diagnostic inspections now explicitly request a fresh wizard picture;
continuation paging is untouched. Exact effect, build, held-XP and RNG assertions
remain. The same four recorder diagnostic entry points were corrected together.
One actual growth-case execution failed at this stale inspection-basis root.

The fifth recorder pass reached Detection but the unchanged 8 GiB disk guard
stopped it after repeated background map pages produced roughly540 MiB of full
headless diagnostic-state logs. This is an infrastructure interruption, not a
failed gameplay method. The Detection clients now opt into existing compact
background diagnostics: raw messages and hydration status remain exact, while
repeated derived-state copies are omitted. After hydration, a local Inspect
provides the full memory for the unchanged content assertion, without a server
Snapshot/reset. A focused helper witness verifies both compact and already-full
paths and forbids an unnecessary request on the latter. Failed logs are retained
in hash-verified compressed archives; no production budget or workload is reduced.

A second recording attempt reached the same disk guard despite compact background
diagnostics; full demanded pictures still made the large Detection lifecycle log
substantial. Both interruptions are one resource-headroom root cause, not two
gameplay failures. Completed release-example executables and failed logs were
hash-verified into compressed archives to provide sufficient recording headroom.

The final recorder completed after headroom recovery; no production thresholds
were changed. The archive writer's initial local selector used a source-file
module name rather than its registered `storage::schema::scenario` path. The
nonempty-test guard rejected the zero-test run. The compiled binary was reused
with its hash and actual `--list` paths verified; seven exact checks then passed.
This is a local driver-selection error, not a production defect or a passing test.
Read-only audit also found two benchmark examples selecting an older wire fixture;
both now select the current fixture and their existing contract tests pass.

### Performance PR hosted 3D formatting gate

The first independent 3D CI run rejected rustfmt layout in two migrated UI
files before compiling or executing behavior tests. This is one formatting
root cause, not a gameplay defect or stale behavioral oracle; platform/profile
repeats are not additional defects. Rustfmt corrected only whitespace in
`interact.rs` and `screen.rs`. Both workspace formatting checks passed locally;
behavioral evidence is reused for identical non-whitespace source, while hosted
native 3D qualification must still run on the corrected head.

### Performance PR causal/display migration: hosted feedback

The first behavioral matrix for [PR #203](https://github.com/KCall360/thresholds-of-ruin/pull/203)
exposed both real client defects and incomplete fixture migration. This is an
ongoing correction window, not a green final-head qualification. Repeated
Windows/Linux and debug/release failures are not additional root causes.

Confirmed client defects include automatic ASCII running choosing its next step
before character readiness, terminal ASCII input being admitted between the
ordered terminal outcome and the later readiness update, and scripted Text
commands returning at causal completion before their demanded picture arrived.
The last case reproduced as successful pickup narration followed by an empty
inventory. An explicit Observe process check also received its picture but timed
out waiting for a correlated reply: picture-only completion was not recognized
by the generic pending-request path. These four roots remain distinct from test
maintenance; their exact safety assertions and process witnesses are retained.

Fixture maintenance includes state annotations still encoding a revision instead
of a snapshot capability, cross-attachment comparisons treating local picture
identities as global identities, and callers expecting a causal-only Wait to
produce an automatic picture. Payload comparisons preserve every disclosed field
and omit only the top-level snapshot/revision identities across independent
attachments. Independent review restored an exact same-picture supply-picker
assertion. The 220 audited comparison sites are source sites, **not** 220 observed
failed tests. A helper requiring fresh post-action inspection requests an ordinary
Observe; raw command delivery and transport lifecycle tests keep their distinct
asynchronous contract.

Maintained direct-Engine benchmark producers also used an internal unissued
picture revision as a change signal: physics emitted zero apply/draw samples for
864 commands across 24 rows; combat did so for 192 commands across three rows.
Item/place/latency additionally retained the old combined event/picture envelope,
and direct place setup incorrectly invoked a display-capability resolver without
Service validation. These are five producer subcases in three failing Python
methods, not five methods. Producer migration must retain report bounds and
separate outcome timing from picture timing rather than rename their sum.

The earned-route driver inspected advancement using a pre-settlement picture.
Its corrected accounting boundary requests one nonresetting Observe, with pure
regressions preserving paid-action counts and error propagation. A subsequent
local process attempt timed out during scenario validation before route execution;
that setup failure is not evidence of either a route regression or a passing fix.
Final correction results and additional diagnosed roots will be recorded when
this bounded batch has completed.

Reusing the immutable, already-built Text and protocol test binaries exposed
later failures that the hosted Cargo run had not reached after ASCII failed.
Text reported 178/199 passing and protocol integration 274/293; protocol library
58/58 passed. These are historical-snapshot diagnostic results, not qualification
of the corrections. The 19 protocol failures have three stale fixture roots:
17 failing methods shared two reply-context constructors that omitted the required
snapshot, one field-list oracle
still counted five fields, and one malformed-delta test accidentally changed its
snapshot base before reaching the intended payload-validation failure.

The Text failures revealed two further genuine roots: converting a noticed actor
to generic discovery lost typed newcomer identity/direction (and therefore
inter-leg caution), and starting play without a welcome call left the first HP
loss without an initialized accounting baseline. Event-time disclosed positions
are retained rather than reconstructed from a later picture; the exact newcomer
stop and two-loss overflow-safe accounting assertions remain. Autonomous Text
chains also confused queue admission with character readiness, the same semantic
readiness root as ASCII autorun above. A shared causal readiness value follows
ordered pacing facts independently of an older picture. Other Text fixtures
still combined events with pictures or authored obsolete snapshot identities;
those are corrected separately from the preserved production safety assertions.

The bounded correction run passed all 160 ASCII tests and all 294 protocol
integration plus 60 protocol-library tests. Common-client tests reached 177/181:
two synthetic `Snapshot.state` constructors omitted `StateView.snapshot`, affecting
four methods (not four production roots). Text reached 197/199: the remaining
two exact total-request counts included a legitimate read-only Observe. Their
revised oracles distinguish the precise picture demand from the unchanged paid
journey/attack/pickup sequence and preserve refusal/no-retry assertions.

The historical server diagnostic was interrupted after 245 passing and one
failing test to avoid spending the local checkpoint on unrelated expensive
floor/recovery cases; the remaining 1,022 were not run. Its one failure was rerun
alone: the roaming-work oracle expected one scene construction, while the retained
optimization performs zero. The corrected oracle still checks the exact actor and
move, no route search, and bounded personal-history copying. The partial suite is
not a pass. A later local lint failure caught an unnecessary unwrap in the migrated
latency producer before application linking; the direct pattern match replaced it.

The existing genuine wire recording contained no Noticed fact. A bounded ordinary
door-reveal process captured the new event-time position from the actual server
in two seconds; the prior 73 samples remain unchanged and this one raw sample was
appended. This is a targeted additional recording, not a claim that all recording
scenarios ran again. The new strict protocol sample regression passes.

The completed focused client/protocol correction owners now pass 894 tests
(181 common, 160 ASCII, 199 Text, 294 protocol integration and 60 protocol library).
All five repaired producer smoke/selected cases execute and pass their existing
report validators: physics 8 rows, combat 3, items 20, places 101 and latency 236.
These development-profile checks qualify report/behavior contracts, not new
performance claims. Three of seven actual-process methods passed immediately;
the four remaining methods exposed narrower stale caller expectations: an opaque
unknown snapshot expects StaleContext, a measured attack must demand its picture
before comparing a later annotation response, place spectators must receive exact
public history rather than wizard-private setup entries, and Text Wait narration
uses its canonical sentence rather than the old debug event name. Both place
variants are subcases of one method. The subsequent corrections keep exact
history, causal timing, paid-action and payload assertions.

A read-only audit also found additional server fixture omissions before another
hosted round: three required snapshot contexts, five comparisons between separately
issued pictures, one comparison mixing private causal and display revisions, and
one equipment-projection check still expecting an unsolicited picture. These are
source-identified corrections, not additional observed failed-method executions.
The equipment check explicitly demands its picture and retains private-curse
redaction checks on both causal facts and displayed fields; it does not require an
otherwise empty private event to be broadcast.


The final selected server checks passed 13 methods across the bounded correction
runs. One exposed a genuine chart-ownership regression: first-page preparation
repeated the already-owned current-boundary census. Reusing the exact current
Game/branch view restores the original Arc-identity oracle without another cache.
A historical first transfer reconstructs from its frozen boundary and owns its
continuation; the regression also checks exact historical charts and unchanged
checkpoint/RNG/knowledge state after the world advances.

All seven targeted process methods above now pass. The saved-exploration retry
also exposed a stale execution-time oracle: a Wait history entry records its
execution/start tick, not its next-ready tick. The replacement binds a new own
Wait receipt by actor, branch, event and exact execution tick, rejects old history,
and preserves exact restart payload/history equality and Text narration order.
This is one further stale driver root, not a production regression. The final
process checks used the previously frozen application binaries; the later pure
chart-ownership fix was qualified by its rebuilt server test owner, not relabeled
as part of those application artifacts. Hosted CI builds the exact final tree.

Resource-guard interruptions during local linking/freezing are infrastructure
failures, not test defects or passing checkpoints. Completed application hashes
were verified after bounded reversible archive cleanup; successful evidence was
preserved rather than rerunning unaffected owners. No guard threshold was lowered.

### PR203 second hosted correction cohort (2026-10-29)

The first correction head's exhaustive Linux server owner reported 1,222 passes
and 50 failures. A bounded local correction run subsequently passed 57 selected
methods and left 18 failures; those successful methods are retained rather than
repeated wholesale. Four of the remaining failures exposed a genuine missing
wrong-branch guard on private queries. The guard belongs after exact retry
recognition and before query dispatch; its existing negative tests and the
exact-once retry regression qualify that ordering. Thirteen remaining methods
still supplied causal revisions to a display-bound inspection target, and one
used a retained valid picture as an invalid-picture fixture. Those are two stale
fixture roots, not fourteen production bugs.

The full process partition reported 105 failures and three errors across 709
executed methods. These are execution counts, not independent defect counts.
Additional genuine client roots identified by those traces are: opening a new
private advancement Overview from a retained picture rather than explicitly
refreshing it; routine own movement being narrated as a caution and stopping
ASCII directional running; and an unnamed newly noticed figure producing empty
Text narration. The correction retains exact attachment/control binding during
Overview refresh and does not refresh already pinned pages or confirmations.

Stale driver roots include comparing newly issued picture capabilities across
restart, reading combat outcomes only from pictures, expecting old debug event
labels instead of semantic narration, and treating causal completion as a fresh
picture. Native input completion now checks the client's disclosed picture-current
flag and authoritative readiness, including frames arriving during key release.
The earned-route restart check excludes only the new attachment's snapshot and
display-generation identities; payload, tick and history remain exact, with
negative mutation witnesses. Dedicated resource probes explicitly hold an
external actor boundary to inspect paid preparation. The shipped autonomous arena
is not changed: its intermediate Falter payment oracle remains in the existing
paused, same-build preparation test, while ordinary execution retains its final
payment and outcome checks.

These corrections are pending final rebuilt process qualification; source review
and pure helper passes do not constitute a green application run. Native physical
key qualification also requires the xdotool dependency; an unavailable dependency
is an infrastructure/setup failure, not a behavior pass or a new waiver.

The final remaining diagnostic case was initially attributed to its old picture,
but failed again after an explicit Observe. That correction disproved the fixture
hypothesis: runtime capture settings/records were projected from a frozen gameplay
boundary even though capture configuration is deliberately not journaled gameplay.
The fix validates the issued capability but reads the current private diagnostic
owner, matching capture controls. The existing regression retains the same picture
across capture/inspect, checks invalid metadata and unauthorized recipients, and
asserts unchanged gameplay state/history. This is one genuine production root,
not another stale-test count. The affected client owner checks passed 642 tests;
this final server correction still requires its own rebuilt witness.

The runtime diagnostic regression then passed, and the ordinary applications were
rebuilt from that corrected source. Eighteen of nineteen representative process
methods now pass, including the paid resource/fear/Falter witnesses, owner
advancement refresh, cancellation, ordered strikes, vampire loot, fruit, and the
Amulet route within its unchanged output budget. Three narrow correction mistakes
were caught and fixed without rebuilding: diagnostic memory revisions are numeric
rather than wire decimal strings; the diagnostic observation-base repeats the
independently issued picture capability; and consuming the last fruit retains the
legacy generic start label followed by its exact named completion. A vampire
purchase-preview equality also needed the same issued-picture distinction.
These are stale fixture/driver corrections, not additional production defects.
The remaining earned-route local run stops in candidate validation at the existing
30-second setup limit with the development opt0 scenario tool. No route behavior
was qualified by that timeout, and no limit was increased.

The next hosted run passed the server library and reached the previously blocked
integration owner: 234 methods passed and 36 failed. Source triage found additional
fixture migrations, not 36 new production defects: trusted direct-Engine helpers
did not bind the new display-operation variant to a causal boundary; transport
fixtures expected eager pictures instead of ordered Outcomes; reset comparisons
included issued picture identities; a note retry reconstructed a different anchor;
and direct performance drivers used the deliberately unissued display sentinel as
an advance counter. Corrections preserve exact event ordering, attachment-local
delta bases, original retry anchors, payload, hidden-observer disclosure, payment,
and causal advancement. Physics now separately asserts no eager picture work and
actual disclosure on demand. These integration corrections are pending their
focused owner run; the prior passing library/client/application evidence remains
valid for unchanged production code.

The next process triage identified additional genuine narration regressions:
ordinary ASCII HP/preparation updates became caution messages; Text retained its
HP/name baseline across a rewind Snapshot; Text no longer used legacy noun
fallbacks and classified refusal sentences as conditions; and observer door facts
omitted their already-authorized event-time direction. The semantic projection
audit also found missing disclosed actor assets (used by Text's palette fallback)
and coalesced ASCII underfoot prose. These are production roots, not stale-test
counts; the correction preserves the disclosed event-time facts rather than
looking them up in a later picture. Focused regressions and rebuilt qualification
are pending for this coherent correction.

Windows release's earned-route execution reached its final comparison after
1,257 seconds and differed only in the independently issued snapshot capability;
Linux likewise differed in picture identity fields. The comparison now excludes
only top-level snapshot/display-generation identities, retaining exact payload,
causal history, gross flows and source-free restore. Sixty-one pure comparison
checks pass; this does not substitute for the next final-head hosted route run.

The focused correction checkpoint passed all 96 selected protocol/client methods
and 269 of 270 server integration methods. The remaining integration failure was
one stale palette expectation: it required one combined delta where event-time
asset dependencies now require additions, the ordered Outcome, then removals.
The replacement checks both exact deltas and the intervening event for player and
spectator, rather than ignoring extra messages. A newly authored palette test
also used an untyped TOML mutation on a typed Archetype, caught at compilation;
this is test-implementation maintenance, not a gameplay defect. Its corrected
fixture uses the existing concealed dual-use archetype rule: globally forecast
item-pool assets do not imply that the actor's regional asset is already present.

The completed correction checkpoint qualified all 270 server integration methods,
the compact/full projection oracle, both palette Service fault/order cases, and
the 96 selected protocol/client methods. The genuine wire-66 recording was
regenerated with the corrected event-time fields. Four additional stale process
assertions were corrected: one restart compared a session-local display revision,
one wizard setup command lacked a fresh observation, one required two transport
frames for two correctly ordered strikes in a single Outcome, and one expected
obsolete readiness prose. Their four actual-process regressions now pass.

Local lossless segmented capture exposed a harness qualification limitation:
raw-file byte budgets do not count archived segments. Such runs establish behavior,
not those byte budgets. The wraith witness was rerun with ordinary raw capture and
its existing compact-background diagnostic option (all wire messages and gameplay
frames retained). It passed the unchanged 96 MiB output bound with 85,917,443 bytes,
249 requests and 10,828 actor ticks, preserving the survived drain and source-free
restart. Hosted ordinary-capture checks remain required for the other budgeted
process cases. Storage-guard interruptions and a mistyped pure-test selector were
setup failures, not production defects or passing behavioral checks.

The regenerated recording's existing Noticed golden test also required updating:
the new foreign-door witness legitimately records a second reveal at the same
exact disclosed offset. Its replacement checks both reveals and the exact new
wooden-door name, open state and observer-relative position; it does not drop the
cardinality or position assertion. The other 17 recorded-sample tests passed
before this isolated expectation correction.

The next hosted Linux debug Rust run passed 2,264 simulation unit tests and exposed
two additional stale work-counter assertions. One still interpreted the historical
`actor_table_copies` field as whole-map detaches rather than per-actor payload
copies; its replacement checks both payload copies and ordered table-entry copies,
while retaining old-snapshot, injury and unrelated-creature assertions. The other
expected repeated classification on an unchanged warm observation; its replacement
checks exact cold counts, zero additional warm classifications, identical disclosed
views, and renewed classification after an actor is added. These are two stale-test
roots/methods, not evidence of gameplay or privacy regressions.
The same two failures reproduced in hosted Linux release (2,264 other simulation
unit tests passed). Both corrected counter contracts passed the focused local
owner; the unchanged production binaries and previously qualified behavior
checks were not rebuilt or relabeled.

The same hosted correction exposed three additional pure-fixture roots: the owned-HD
wizard double still accepted only Snapshot where setup now explicitly requires
Observe; the active-mob scripted binary bundle omitted Windows executable suffixes;
and the combat driver double put completion receipts in picture events rather
than ordered Outcomes. These account for six existing failed methods (one wizard,
one driver and four combat methods), with repeated platform executions and combat
subtests not counted again. Nine focused wizard/driver checks and eight combat
checks pass, including both platform path branches and a negative picture-only
completion witness. Production behavior and hash/identity/deadline checks were
not relaxed.

The real ASCII return-journey assertion also read a picture before its refresh:
authoritative arrival correctly preceded the new displayed tick. Its replacement
uses the outbound journey's existing current-picture barrier and retains the exact
return tick. The door rewind witness likewise relied on display pace to keep
travel unfinished; its replacement holds the Text client's arrival receipt while
an independent spectator verifies completed travel and a still-closed door, then
rewinds before releasing the pending door goal. These are two stale process-test
methods, pending focused actual-process qualification; no additional production
root is established by these timing failures.
Both actual-process replacements now pass on the unchanged qualified applications.
The first travel rerun additionally exposed a fixture-owned transport deadlock:
it held the command acknowledgement, while automation intentionally waits for
acknowledgement before reading another key. The relay now delivers Active and the
acknowledgement before holding later steps, preserving the Skip control, seven
paid steps, ordered history and source-free restart checks. This is maintenance
within the same travel method, not a production defect or another failed method.
The door witness drains each completed prompt explicitly before its query barrier.

The following hosted Linux debug run passed all 1,274 server unit, 270 server
integration, 2,266 simulation unit and 815 simulation integration tests, then
reached twelve failing test-support performance methods. All twelve stopped while
decoding one shared synthetic view that omitted the now-required snapshot identity;
this is one stale fixture root affecting twelve methods, not twelve engine bugs.
The repaired fixture adds the identity and a required-field rejection witness.
The forbidden-acceptance mutation also advances its revision so it reaches the
intended acceptance guard rather than the independent progress guard first.
All thirteen focused performance-oracle tests pass, including their exact expected
failure reasons. No production oracle or gameplay logic changed.

The same hosted round's Linux release Python lane exposed one further stale
private-report caller: the helper refreshed before creature inspection but not
combat inspection after causal-only actions. It now explicitly observes before
either private query, without retrying a rejected command. The failing fear,
disadvantage, immunity and source-free restart witness and the outgoing-policy
restart sibling both pass on unchanged qualified applications. Count one further
stale method/root; none of its RNG, payment, trace, privacy or restart assertions
were removed. Windows release Python shard 2 completed successfully on the
published test-correction head, including the preceding travel and combat fixtures.

The Linux debug Python lane also exhausted its 40-minute job budget without a
failed test: its exact receipt records 2,049 passed of 2,395 selected, leaving 346
unexecuted. This is incomplete infrastructure qualification, not a test pass or a
new gameplay defect. The two longest completed witnesses took 1,251 and 714
seconds. Linux debug now uses two existing module-owned shards, placing those
witnesses in separate owners while retaining the same full inventory, original
per-test limits and 40-minute job cap. The aggregate independently checks exact
coverage, source/run/attempt bindings and both compiler identities; Windows and
Linux release coverage are unchanged.

All 161 owning orchestration methods qualified across focused receipts. One stale
workflow-name expectation was updated for the explicit shard owners. Five CLI
plan subcases initially lacked the actual local Rust toolchain environment; the
two affected methods passed after supplying it, without compilation. These setup
failures do not count as production detections. Independent review caught and
corrected two new orchestration defects before publication: the seal's strict
output-key set needed the shard-attempt fields, and each shard needed a distinct
compiler role instead of a last-writer matrix output. Negative regressions retain
both safeguards. Actionlint, Ruff and the focused coverage/attempt/seal tests pass;
final hosted exhaustive qualification remains required.

Once the full Rust suites passed, their downstream production Service catalog
owner exposed seven stale methods across three contract migrations: two still
mutated removed action display revisions instead of input authority generations;
four compared newly issued picture identities/generations as immutable payload;
and one expected an unsolicited hallucination picture instead of causal visual
invalidation followed by display demand. These are three fixture roots affecting
seven methods, not gameplay defects. All 29 Service methods now pass against the
exact Cargo-produced current host, including exact stale/wrong-branch errors,
hidden-hold retry/no-retarget behavior, complete disclosed payload comparisons,
private discovery and exact hallucination phase samples for owner and follower.
No production code or privacy/replay assertion was relaxed.

Linux release's projectile process exposed one additional stale decision fixture:
its headless peer had settled equipment, but Text still held the older picture
when a new launch menu began. Retained stdout proves the client correctly refused
the changed selection without sending an action; the fixture's shortbow substring
accidentally matched preceding equipment narration. An explicit sync before the
new menu verifies one real snapshot and its complete disclosed payload, then keeps
the exact page count, expertise, payment, recovery and source-free restart checks.
The full affected process passes. This adds one stale method/root, not a transport
or gameplay defect. Both new Linux debug Python shard jobs also completed
successfully; aggregate exact-coverage proof and final-head CI remain separate gates.

Windows debug's native bump fixture reached and verified native input, recovery
and unchanged resource balances, then failed its raw private `combat inspect`
query with `StaleContext`. The fixture now uses the existing explicit pre-query
Observe helper, without retrying an action or weakening the exact owned-melee
trace and narration checks. A direct-caller audit found four equivalent new-report
openings in natural-attack, equipped-weapon, replay and data-only ruleset tests;
those now refresh too, while backward paging and deliberate stale-capability
queries retain their existing basis. This is one repeated stale-fixture root,
one observed failing method and four proactively corrected methods, not a new
gameplay defect. All four affected non-native processes pass (22.816 seconds),
as does the helper's refresh-once/no-retry regression. The exact native case
still requires hosted qualification: local X11 lacks xdotool; no local native
pass or waiver is claimed. Production source and binaries are unchanged.

The next hosted run exposed two timing-dependent stale fixtures. Generated-caves
spectating waited for the final move's intermediate started-picture tick, which
an independent native client may coalesce. The helper now waits for a stable
ready/no-pending-action boundary before exact spectator, rewind and restart
observation comparisons. The data-only condition test queried private creature
state after a peer action without refreshing; it now uses the existing fresh-query
helper. These are two observed failing methods/fixture roots, not gameplay errors.
A broader audit also corrected four analogous inspection openings (one unavailable
target negative, two Text inspections and one native inspection after external
expiry); pinned report paging and intentional stale-context negatives are intact.

The affected generated-caves graphical process, data-only condition/restart,
frozen/detached unavailable-target, private paid-preparation and status-protection
processes pass locally. One combined run preserved three successful methods before
the disk guard interrupted its fourth; only that unfinished method was rerun after
hash-verified reversible compression of two superseded server rlib cache objects
reclaimed 659,116,032 physical bytes. This resource interruption is not a test
pass or production detection. The native physical-key status case remains a hosted
gate because local xdotool is absent. No production source or time limits changed.

A later Windows debug run exposed a distinct synchronization defect in the
clarification/rewind fixture: its relay held Text's first travel envelope, not
server execution. A fresh wizard Observe at tick 50 could therefore become stale
while seven authoritative travel steps advanced to tick 750 before rewind. The
fixture now waits for the independent wizard stream's matching Arrived status,
asserts all seven steps, and submits one fresh-context rewind while Text is still
held and cannot perform its compound pickup. Clarification remains byte-for-byte
free, pending pickup is cleared, and exact branch/tick/inventory/reconnect checks
remain. The actual affected process passes locally in 1.729 seconds. This adds one
observed fixture synchronization failure, not a gameplay defect; no retry, timeout
increase or production change was introduced.

## ASCII NetHack-controls help regression

[PR #205](https://github.com/KCall360/thresholds-of-ruin/pull/205)'s first hosted
Linux debug and release Rust partitions exposed one presentation regression:
the compact help rewrite retained F7's wizard-editor label but omitted the
existing creature-list hint. The unchanged
`presentation::wizard_creature_catalog_is_discoverable_in_ascii_help` assertion
failed in both profiles; 177 other ASCII integration methods passed in each.
This is one production discoverability defect and one distinct failing method,
not two defects or an infrastructure failure. The correction restores the
creature-list hint on the F7 line without changing the key binding, behavior or
assertion. Final-head hosted qualification remains required.

The same run's Linux debug native door/restart witness exposed one missed test
fixture migration: its final Escape still expected process exit, although the
new documented contract keeps the map open. The fixture now sends native Shift-S
through the same OS-key path and retains its exact saved-door, spectator and
restart assertions and timeout. This is one stale fixture root/method, separate
from the help defect; it is not an unintended product quit regression. The new
native Search/extended-command/spectator witness passed in that lane. Local
physical-key qualification remains unavailable without xdotool; the corrected
native door owner must pass hosted CI.

Windows debug and release additionally timed out in the new ambiguous-command
native witness while waiting for exactly `k`. The fixture sent both a held
KeyDown and an explicit character; minifb's Windows event loop already calls
[TranslateMessage](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-translatemessage),
which translates the key into a character. Source tracing identifies duplicate
character injection as the fixture cause; the retained diagnostic tail does not
include the beginning of the final frame, so it is not direct evidence of the
editor's full text. The corrected fixture uses one held native K on Windows
and the existing native text route on X11. It asserts the first nonempty edit
is exactly `k`, then retains the ambiguous-Enter, unchanged-state and paid-Search
checks. This is one additional fixture root/method, not a verified product
defect. Final Windows native qualification remains required.
The cheap injection-ownership regression failed first on the duplicate Windows
text call, then passed with the single-path correction. Companion checks retain
Windows key release on a frame-wait failure and immediate rejection of a `kk`
edit. These mocks prove fixture ownership/cleanup, not Win32 translation; the
real native acceptance remains the platform authority.

## Live Rogue content activation (2026-10-29)

The approved data-only activation intentionally supersedes earlier opt-in-only
stock and exact thirteen-stage assertions. The first content checkpoint had
18 distinct stale methods (19 failure reports because the dart/shuriken method
reported two subtests); two diagnostic-adapter methods then exposed a stale
listed-stage inventory and a last-stage `bands` assumption after the appended
treasure stage. These are planned policy/fixture maintenance, not discovered
production gameplay defects. Existing item/effect, unchanged-kit, legacy-stage
and historical-oracle assertions remain; explicit live-policy and real-process
birth/layout/reopen witnesses cover the new policy. The three deliberate
feature-first-red tests are excluded from stale-test and defect totals.

The first combined process checkpoint overlapped a server test-binary build and
hit two setup/deadline failures: the unmodified-live-package witness exceeded its
60-second compiler limit, and the existing preset witness exceeded the Mage's
server-start deadline. The suite was stopped (exit 130), retaining both failures
and the completed all-17 and existing treasure-room witnesses. Resource
contention is a possible contributor, not a proven cause; neither failure
establishes a gameplay defect. Failed/interrupted and unrun selected methods are
rerun serially after the Rust checkpoint, without increasing limits or weakening
assertions. These two distinct methods are separate from the stale-oracle counts
above; interrupted methods without outcomes are not counted as failures.

Both earlier deadline methods passed serially with unchanged limits. Two other
methods then exceeded existing 30-second fixture-compilation limits before any
play: Mage source-deletion and the fixed-initial first-floor walkthrough. The
qualified local compiler is dev-opt0; unmodified live-package validation passed
in 51.4 seconds, so these capped methods remain locally unqualified. Ordinary
death was reached, but its saved-game reopening exceeded the existing 15-second
startup deadline after `restore.replay.begin`; that replay outcome also remains
unresolved. No crash or gameplay diagnosis follows from the deadline alone.
There are five distinct failed/error methods across these checkpoints, two later
passing and three unresolved locally. On maintainer direction, the selected
four deep-fixture compiles and unfinished technique methods were deferred to
required hosted profiles rather than repeating predictable compilation limits
or starting an optimized-build campaign. No timeout or assertion was relaxed;
these deferrals are not passes or graphical waivers.

### PR #206 hosted fixture and driver corrections (2026-10-30)

The [first hosted run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40975162876)
for [PR #206](https://github.com/KCall360/thresholds-of-ruin/pull/206) exposed
**five distinct failing methods**, not five production defects. This window has
four fixture/driver root groups: supplemental-stage count, omitted/maze layout
migration (shared by two methods), mixed-room band shape, and walkthrough endpoint
synchronization. The first three are stale/unupdated fixtures; the last is a stale
driver assumption with harness overlap. Debug/release repeats are additional
executions of the same methods and causes, not new roots. No complete failed-
execution total is inferred from selected logs. The walkthrough also overlaps an
earlier local setup-timeout method above; it is not a new method in a cumulative
union of those windows.

| Distinct failing method | Established cause and scoped correction |
| --- | --- |
| `scenario_package::tests::rogue_magic_catalog_uses_explicit_capabilities_and_appends_finite_stock` | The legacy loop expected every appended stage to have one item attempt. Keep exact legacy-stage checks and independently assert the supplemental 0–1 range, all 17 equal-weight choices, and the mixed treasure policy. |
| `scenario_package::tests::bundled_rogue_uses_compact_open_family_and_preserves_room_recipe` | The live recipe now lowers to version 5 with omission/maze stages, not the discovery-only version 3. Pin the live stage order and operands; a validated control removes only those two stages for the retained historical geometry/discovery comparison. Live geometry is separately allowed and witnessed to differ. |
| `test_rogue_roster_package.RogueRosterPackageTests.test_production_retrieval_uses_one_selected_player_bound_ordinary_amulet` | A universal `stage['bands']` lookup failed on `room_cluster`. Dispatch explicitly to required ordinary `bands` or both `item_bands` and `actor_bands`; preserve Amulet exclusion from all stages, starting loadouts and carried pools, without silently skipping missing fields or unknown stage kinds. |
| `test_rogue_exploration_process.RogueBundledPolicy.test_geometry_fixture_preserves_the_population_free_workload_explicitly` | The same layout migration invalidated a source-recipe comparison that removed only discovery operands. Independently pin omission count `[0, 1]` and maze `one_in = 15` before removing those fields; retain the exact baseline, absent-feature, population-free and character assertions. |
| `test_rogue_playable_process.RoguePlayableProcesses.test_ordinary_first_floor_foes_earn_second_hd_and_ground_supplies_are_finite` | The driver assumed the returning PC's action was the durable history endpoint. A legitimate hidden bat `MoveWest` completed in the same tick after the PC's `MoveUp`, so the private endpoint correctly differed from the last public entry. The correction uses one unconditional ordinary paid Wait on the verified-cleared returned floor before the unchanged strict visible-endpoint seal; stock, XP, location and entry-receipt guards remain. |

The two exact Rust regressions passed after input-stable preflight and linking.
Both Python failures were reproduced locally before correction, then all eleven
methods in their two pure owners passed. All 24 pure movement/transport checks
also passed, including positive one-Wait behavior and rejection of unsafe input,
unpaid or moved results, and changed stock. These are scoped local results, not a
new full-suite or final-head hosted pass. No production behavior, timeout,
visibility rule or strict endpoint assertion is changed by these corrections.

Hosted execution also passed the previously local-capped Mage source-deletion
and ordinary-death reopen cases, plus actual treasure, all-17-item, technique and
deep-fixture cases. Those outcomes resolve the earlier hosted deferrals, not the
recorded local failures. The corrected walkthrough still requires a hosted
actual-process rerun: local dev-opt0 fixture compilation hits the unchanged
30-second cap before gameplay. No corrected walkthrough pass is claimed, and its
remaining qualification is not replaced by the pure-driver or Rust results.

## PR #207: native fixture races and post-test job budget (2026-10-29)

The initial [3D CI run](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40988267824)
exposed two fixture causes in two native methods, each failing in Linux debug and
release: four confirmed failed executions, not four defects. The map/journal wait
called `startswith` on a legitimate transitional `screen: null`; the HUD's empty-
dock witness requested physical x=1411 on the default 1280-pixel Xvfb desktop,
which correctly clamped the pointer to x=1279. Both the press and finally-release
errors belong to that same HUD execution. No production gameplay defect follows
from either result.

The map waits now treat absent screens as not-yet-matched. The blank-dock witness
uses bottom center, still below the buttons, and retains exact revision/position,
mouse readback, own-window, button release and no-click-through assertions.
`NativeMouseContract` exercises the actual native test methods with independent
valid frames and a 1440x900 client on a clamping 1280x1024 fake desktop. Its two
regressions first reproduced the exact CI failures, then passed; these mocks do
not qualify physical OS input. The native methods remain mandatory in hosted CI.
Initial isolated-regression setup omissions were corrected before the intended
first-red evidence; they are authoring/setup failures, not additional product or
stale-fixture causes.

Separately, the [Windows debug Python shard](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40988267891/job/123848053534)
completed verification successfully (2028 seconds), uploaded its evidence, then
hit the 40-minute whole-job limit during Rust-cache cleanup. The Windows
aggregate correctly rejected the cancelled cohort. This is one infrastructure
budget cause and one cancelled job, not a failed test method or passed full gate.
The Windows Python job now allows 50 minutes, retaining all per-test/verifier
limits and fail-closed cohort validation. An independent workflow regression
rejects the old budget and requires bounded post-verification headroom. Fresh
final-head hosted qualification remains required; earlier cancelled/failed jobs
are never relabelled as passes.

### Later Windows release evidence from the same run

The Windows release job completed during the first correction's publication.
Its HUD witness also hit a clamped pointer, this time the Inventory button at
`(195, 813)` on a 1024x768 desktop, returning `(195, 767)`. This is the same
undersized-desktop fixture cause, not another gameplay defect. Its advancement
method also ended at 604.5 seconds while typing the final ledger command; the
fixture passed `--quit-after 600`, and the retained frames still showed a live,
nonfatal advancement screen. The fixture's application lifetime cap prematurely
terminated a legitimate slow software-rendered run.

This adds one distinct failing method and two confirmed failed executions to the
Linux window above: three distinct native methods and six failed executions in
the observed completed lanes. The remaining old Windows debug lane is not
inferred to have passed or failed. The new advancement lifetime regression rejects
the self-exit argument; the normal harness and explicit Escape/30-second quit
assertion own shutdown. The existing 90-second frame waits, 3000-second native
suite bound and all purchase/ledger/typing assertions remain unchanged.

The independent workflow now establishes adequate physical desktop space before
native verification: a supported Windows mode of at least 1600x1000 and a
1920x1080 Xvfb screen. The Windows helper preserves an already-large desktop,
checks mode-change success and actual readback, rejects unsupported/small modes,
and changes no persistent display settings. It runs only on disposable Windows
GitHub Actions hosts, never against a local maintainer desktop. Unit contracts
cover the Win32 ABI, mode selection, rejection/readback, local-invocation guard,
workflow ordering and actual native fixture launch arguments. These contracts
passed locally; live display setup and OS input remain hosted qualification.

### Advancement reply ordering during the earned route

The [Windows release Python shard](https://github.com/KCall360/thresholds-of-ruin/actions/runs/40998634565/job/123880989177)
failed `test_fixed_seed42_earned_hd_reexecutes_with_exact_source_free_restore_and_gross_flows`
with `Advancement page does not match its exact request.` Its retained headless
transcript shows an issued picture at revision 4, followed by an ordered outcome
advancing the event cursor from sequence 10 to 11 without replacing that picture.
The pending advancement query incorrectly required the old complete reply
context, although ordinary commands carry input authority and expected picture
revision, and replies describe the attachment state when published.

This is one shared-client production correlation defect, one distinct failing
process method and one observed failed CI execution. It is not a route fixture
failure or grounds to relax the earned-route assertions. The other observed
platform/profile route passes do not qualify this failed execution. The Linux
aggregate correctly rejected the failed Windows cohort.

The unit regression `pending_pages_survive_ordered_cursor_progress_on_the_same_picture`
and the existing socket acceptance owner extended with an interleaved outcome
both first failed with the same exact-request error. The correction allows only
ordered cursor progress while retaining the exact picture and input authority.
Negative coverage rejects changed actor, branch, stream, reset, readiness,
snapshot, revision, backward cursors and control loss. Existing exact displayed
page freshness and both-presenter control-revocation witnesses remain strict;
the connection still validates the full current wire reply context. The unit
owner also covers a pending ledger query with a retained overview, where the
old observation path discarded the pending request.

An initial local preflight omitted the integration target's `test-support`
feature; the corrected feature configuration compiled. This setup failure and
the deliberate first-red executions are not additional production root causes.
Final-head hosted route and full qualification remain required.

### Expanded Windows native suite exceeded its aggregate budget

The next head passed both core platform aggregates and fresh full PR
qualification, including the Windows release earned-route shard. The independent
[Windows debug](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41005205490/job/123898652846)
and [Windows release](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41005205490/job/123898652879)
3D jobs then exhausted the process suite's 3000-second budget. The debug receipt
records exit 124 after 3000.063 seconds, with 21 passed and 14 skipped methods out
of 57 selected; release records exit 124 after 3000.016 seconds, with 22 passed
and 15 skipped. Neither timing stream contains an assertion-error diagnostic.
The remaining selected methods were interrupted or never reached, not passed.

This is one aggregate infrastructure budget cause and two failed suite
executions, not two failing native methods or a demonstrated gameplay defect.
The expanded UI coverage consumed the old allowance while making valid progress:
graphical advancement alone passed in 858.7 seconds on debug and 827.3 seconds
on release. Debug compilation and setup consumed approximately forty minutes
before process acceptance started. No claim about 100-creature performance
follows from these software-rendered test timings.

Two configuration regressions first rejected the old suite/job allowances.
Windows process acceptance now has a bounded 90-minute suite allowance, and its
job allows 150 minutes for compilation, acceptance and artifact cleanup. Linux
keeps its 50-minute suite and 120-minute job limits. The existing timeout
regression still requires nonzero failure and retained diagnostics when the
suite deadline expires. Individual frame waits, quit deadlines, test selection,
native assertions and hosted no-waiver rules are unchanged. Fresh final-head
hosted native and core qualification remain required after this control change.

### Portrait scale measured colored clothing instead of the figure, 30 October 2026

The documentation-only PR208 passed its exact-head core and independent 3D gates
before merge. On the identical merged tree, the independent
[Windows release job](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41085519415/job/124165532111)
failed only
`Native3DProcesses.test_character_screen_shares_the_live_portrait_and_keeps_command_keys`,
in its `character_scale` subtest: 0.5084745603469354 was not greater than 0.52.
The suite completed 57 methods with one failure and 29 existing skips; the other
three independent 3D jobs and core post-merge checks passed. This is one
demonstrated screenshot-oracle root, one distinct method and one failed hosted
execution, not a newly demonstrated renderer defect or a waived failure.

The retained screenshot visibly contains the full figure from hair to boots.
The old pixel predicate required both brightness above 55 and RGB spread above
10; dark, nearly neutral trousers and boots therefore did not count. Replaying
the exact predicate on retained shots 06 and 07 reproduces 0.508474560, ending at
the colored cloak rather than the feet. The connected visible figure occupies
0.696798472 of the same image pane. All 17 retained unrotated active captures
show the old false negative; all four rotated captures pass the old predicate.
This does not establish why the prior hosted execution passed, and does not
support repairing the test with an arbitrary settling delay.

The corrected scale oracle remains inside the disclosed portrait image bounds.
It estimates the near-black clear background from the inset perimeter, then
measures one connected contrasting component with more than 1,000 of the
existing bright/color samples. Dark neutral pixels can extend that same body;
unrelated marks cannot be unioned into a larger figure. Edge-touching components
and non-dark uniform backgrounds are rejected. The strict height requirement
remains **greater than 0.52**. The independent nonempty-render, actor identity,
actor-count reuse, texture-limit and heading-spacing assertions remain intact;
no renderer, camera, fixture model, timeout or waiver was changed.

An independently drawn tall figure with dark legs first failed the unchanged
native assertion at 0.395, then passed after the oracle correction. Nine
`test_verification.PortraitSilhouette` methods cover known RGB/RGBA extents,
neutral/dark clothing, translated panes, empty and uniform images, borders,
captions, disconnected noise and marks, undersized figures, invalid bounds, and
the exact/below/above 0.52 boundary through the actual native assertion helper.
Exact background, contrast and connected-seed cutoffs are also covered; two
independently eligible small components must not combine into a large figure.
The complete 34-method 3D tooling owner and all 21 retained active captures pass
the corrected measurement. Synthetic tests and retained-image replays are not
fresh native executions and do not inflate the hosted failure count. The
existing real equipment and character native methods remain the live boundary;
fresh final-head hosted qualification is still required.

Integrated qualification later encountered one runner setup failure, not a test
failure: [Linux debug Python shard 2](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41152209718/job/124391616165)
had only the Microsoft package feed in its updated index, then could not locate
`libx11-dev`, `libxcursor-dev` or `libxrandr-dev`. No tests ran in that attempt.
A targeted retry passed, but the [aggregate](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41152209718/job/124414302702)
correctly rejected Linux shard origin attempts 1 and 2. This is one separate
retry-procedure error, not another failing gameplay method. Both Linux debug
shards must execute in the same attempt; an isolated sibling retry cannot qualify
the cohort. Complete final-head qualification was restarted without changing
assertions or weakening the cohort guard. Native passes from prior heads remain
historical evidence, not substitutes for the final integration's checks.

### Measurement-witness setup and rules-source inventory

The local rules-identity and count-witness checkpoint exposed two test-fixture
causes, not additional gameplay defects. The new source-only comparator name
was missing from the documentation workload allowlist (one failing method in a
39-test Python checkpoint); adding that name preserved historical identifiers,
and all 19 documentation tests then passed. The new scaled Polymorph fixture
attempted to configure an actor already owning combat and correctly received
`ActorBusy` before its measurement (one failing method/execution). Constructing
2/16/64 HD in the original birth builder instead retained every semantic
assertion; all four count tests then passed and produced 16 rows. Existing
ordinary fixtures still construct exactly two HD.

An earlier outcome-test preflight tried to serialize the deliberately non-wire
compact observation type: two compiler diagnostics from one scaffolding error,
not two failing behavioral methods. The correction compares complete typed
inputs and serializes actual eventful outcome facts, without changing any wire
type. Four outcome-adapter tests pass before and after the per-call index,
preserving privacy, occurrence/list ordering and required matched-row emissions.
The count reductions do not establish a release timing improvement. Focused
local feedback is not full or hosted qualification; failed evidence remains
separate from subsequent passing checks.

## Checkpoint failure classification, 30 October 2026

A scoped reduced encoder limit reproduced the explicit-Save retry policy on an
otherwise valid ordinary checkpoint. The new
`storage::checkpoint_failure_tests::unchanged_capacity_failure_keeps_work_and_explicit_save_does_not_reencode`
failed at the intended assertion: the second Save encoded the identical retained
snapshot again (two attempts rather than one). This is deliberate first-red work
for the approved failure-classification policy, not evidence that gameplay has
reached the real capacity ceiling. The SQLite lock/retry and concurrently retained
newer-checkpoint controls passed in that same run.

An earlier preflight failed before behavioral tests: a wrapper made test-only by
the refactor still compiled in production, and new SQLite count assertions asked
for an unsupported Rust unsigned result type. These are two local scaffolding
roots in one failed preflight, with no failed test-method executions. The wrapper
is now test-only and the zero-count assertions use SQLite's signed type; no
behavioral assertion was weakened. Typed encoder controls also distinguish
serializer/resource/writer failures from explicit bounds, preserve the first
failure during buffered cleanup, and recover Interrupted writes without masking
a later real bound failure. Runtime validation and qualification are reported
separately from these failures.

Recovery of an interrupted incremental build later failed in the linker with
undefined hidden LLVM symbols before executing any tests. Isolating only the
affected server-test incremental cache, while retaining the application and
dependency caches, allowed the unchanged implementation to pass all 219 storage
checks on the current main baseline. This is one local build-infrastructure
failure, not an additional production defect or failed test-method execution.
The subsequently interrupted application build was resumed separately; its
verified binaries passed all nine background-save process checks. Interruption
without an exit receipt is incomplete evidence, not a pass or behavioral failure.

## Item appearance and knowledge migration, 31 October 2026

The approved current-only format migration separates persistent public appearance
keys from disclosed identity and independently remembered BUC, enchantment and
condition facts. Deliberate first-red checks exposed incomplete validation of
unselected seeded identities (both reserved/non-leaf keys and inherited-identity
mismatch), a missing restored-knowledge leaf check, a stale name-based wizard
reservation branch, and a client presentation adapter that forwarded injected
private semantic fields. The adapter test is defense in depth: the simulation's
ordinary sampled-presentation path already redacted those fields. An initial
wizard run failed in fixture setup; only the later run reaching the reserved-name
case is behavioral evidence. Failed setup and zero-test runs are not first-red
qualification.

The first integrated server-library checkpoint passed 1,200 of 1,295 methods.
Its 95 failures were classified before repair: 58 shared persona-helper omissions,
8 other explicit identity fixtures, 2 hand-written item DTO omissions, 2 old
cosmetic-equality assertions newly including semantic identity, 1 default fixture
identity mismatch, 1 changed canonical appearance seed vector, 1 obsolete blanket
corrosion-privacy assertion, 1 saved-identity expectation, and 1 pinned region
header/bytes expectation. Nine failures awaited genuine new-version wire/save
fixtures. Ten failures exposed one production diagnostic admission guard still
accepting only fixture version 1 after the authored fixture moved to version 2;
one was the intended wizard reservation regression. These are method counts in
one execution, not 95 independent production defects. The diagnostic admission
fix retains explicit obsolete-version rejection. Seed-vector expectations were
independently recomputed, rather than copied from the implementation's output.

A separate real-server ring process subsequently exposed a new disclosure-boundary
omission: the owner received correct remembered facts, but the spectator also
received the new semantic identity/knowledge fields because the existing sanitizer
only knew the older interaction metadata. The repair covers inventory and ground
items, including the early-return predicate and unchanged public-Arc sharing. It
preserves existing AccessRole policy: Player read knowledge does not depend on a
controller lease, while Spectator receives generic unknown identity and unknown
new facts. Public names, appearance, quantities and existing affordances retain
their prior policy. This is a production integration defect introduced by the new
DTO fields, not an old gameplay bug or stale fixture.

Final rebuilt process, protocol, save and renderer qualification is recorded
separately; the checkpoint above does not claim those later stages passed.

The rebuilt real-server ring test now passes its owner/spectator and source-free
restart assertions. A later certificate refresh passed 44 packages, while three
zero-authored-region procedural packages passed source checking but failed
immutable artifact publication. Their semantic model identity was unchanged by
an internal compiled-envelope bump, so the new writer found the earlier local
intermediate-format artifact at the same address. This is one artifact-address
versioning omission, not three invalid scenarios. The narrow repair binds the
immutable address to the serialized format tuple in a new domain; it does not
replace the old artifact, loosen current-format decoding or change model semantics.

The documentation version guard subsequently caught one stale current-version
table (ContentPackage 33 and RegionInput 9 instead of 34 and 10): one failed
method with two subcases. The current table was corrected; historical qualification
records remain unchanged.

Fresh protocol and model checks found three additional fixture roots: two protocol
cosmetic cases and one model cosmetic case retained a true semantic identity in a
supposedly sanitized input; one protocol named-object oracle still counted eleven
fields rather than thirteen. These were four failing methods, not production
privacy failures. Valid sanitized inputs now use generic unknown identity; the
protocol negative mutations additionally reject a true key, a more-specific
unknown-category key, and a remembered `false` protection fact under a disguise.
All prior real-handle, quantity, currency and navigation assertions remain.
The complete protocol package and all 77 model checks then passed.

Real-process recording also caught an intentionally changed canonical-key seeded
form vector and an amulet fixture reusing the reserved production identity. The
fixtures now state the exact new deterministic form expectations and a distinct
synthetic retrieval key. The amulet observer oracle compares the same spectator
before/after a refusal, rather than equating private owner metadata with spectator
metadata. Targeted utility and amulet processes passed before the genuine wire
recording completed. Additional armor acceptance preserves per-instance facts,
source-free reopening, noncontrolling Player reads and Spectator redaction.

The next rebuilt server checkpoint passed 1,280 of 1,299 methods. Nineteen
remaining fixture failures were traced before repair: eight concrete fixture
items lacked explicit identities, four indexed projectile variants accidentally
shared the base arrow key, four comparisons equated spectator and owner facts,
one encoding-size oracle used the owner's rather than the spectator's state,
one expanded nine-fact connection test exceeded the sixteen-client limit without
releasing earlier pairs, and one built-in tablet key differed from its authored
counterpart. Indexed projectile keys preserve the old archetype-based groups;
the signature validator remains strict. The independent spectator expectation
enumerates private fields while retaining exact legacy public-field comparisons;
same-role equality is untouched. The connection test still checks every fact and
both roles, then releases each pair. All 1,299 methods and all 2,319 simulation
methods passed after the coherent repair.

External integration then passed 267 of 272 methods. Three websocket cases had
four cross-role comparison sites needing the same independently authored
spectator expectation; annotation no-update barriers, cursors, history privacy
and same-user delivery checks remain. One appearance-pool fixture still replaced
an old string-array spelling, so it never added the second entry; it now edits
and verifies the typed TOML records. One actor-asset negative fixture needed an
explicit item identity to reach its intended asset validation. All 272 methods
then passed. Genuine save66 writer output and all 47 actual package validations
also pass; old immutable artifact bytes remain unchanged under the new namespace.
### Arena sampling dependency pins diverged during Alembic maintenance, 31 October 2026

Dependabot PR216 updated the hash-pinned Alembic requirement from 1.20.0 to
1.20.1 without updating the sampler's explicit environment contract. The
[Linux debug Python shard](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41133334551/job/124329687533)
reported five distinct arena sampling/campaign methods failing, including the
requirements/allowlist consistency check. These are manifestations of one stale
dependency-contract root, not five independent production defects. The local
consistency test reproduced the mismatch before repair; no acceptance threshold,
history-integrity check, or environment equality guard was relaxed.

The Alembic pin now agrees with the hash-locked environment. Independent tests
reject an installed 1.20.0 environment and an otherwise integrity-valid history
from 1.20.0, preserving that history unchanged. Both new rejection witnesses
failed before the pin correction and passed afterward. Existing older NumPy and
SQLAlchemy rejection witnesses remain intact. Sampling histories remain bound
to their exact environment; this maintenance does not introduce cross-version
history compatibility. Fresh integrated hosted qualification remains required.

The separate SQLAlchemy PR217 exposed the same requirements/allowlist drift
for 2.1.5 to 2.1.6: its [Linux debug shard](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41133356249/job/124335725929)
reported the same five methods. This is a second dependency-update instance of
the same contract-maintenance mistake, not five more independent defects. The
SQLAlchemy update adds its own immediate-prior 2.1.5 environment and
integrity-valid-history rejection witnesses while retaining the older 2.1.4
controls. Both immediate-prior witnesses failed before its pin correction and
passed afterward. Alembic and SQLAlchemy remain separately scoped maintenance
changes; each final integration needs its own qualification.

### Item migration hosted qualification, 2 November 2026

The first PR221 tooling run reported 45 E402 diagnostics across 13 content-test
modules: the new identity-contract import preceded each module docstring. This
was one structural fixture-edit root, not 45 behavioral failures. Moving the
imports below the docstrings preserved every assertion; pinned Ruff and all 71
checks in those modules passed locally.

The first Linux debug Python process shard independently caught a benchmark
version-oracle mistake: the item geometry fixture advanced to version 2, but the
unrelated place-discovery fixture remained version 1. `place_bench` incorrectly
required version 2 and panicked before running its workload. Its assertion now
again names the unchanged place-fixture version. The existing actual-producer
acceptance test is the first-red witness; its post-fix hosted execution remains
required, not inferred from static checks.

Both Linux Rust profiles reached the same two ASCII fixture rejections (176
other methods passed): a class-changing clone retained the original class's
unknown identity, and a sampled ground-item fixture retained its true identity.
The fixtures now match the already-enforced public disclosure contract. The
validator and all equipment, opaque-target, naming and action assertions remain
unchanged; repeated profile failures are not counted as new roots.

Actual protected-binary checks of the scenario package, source and compiled-input
process modules reproduced 15 failures and one error across 33 methods. Repairs
supply explicit fixture identities/persona corpse keys, replace selectors that
matched pre-migration TOML spelling, and check the actual current region format
10 and its immediately previous format 9. All 33 methods then passed, including
source-location, UTF-8 byte-boundary, source-free restart and no-rewrite checks.

The process partitions exposed outdated comparisons between Player and Spectator
pictures. Directional test oracles now redact only the *expected* Player item's
identity and knowledge and require exact Spectator unknown values; they do not
normalize away leaked actual values. Same-role comparisons remain strict. New
mutants reject known identity, learned zero/false, missing fields and changes to
legacy public fields. Role-proven call sites include readiness predicates,
stream recovery and graphical observations; history, sequence, frozen-state and
no-partial-publication checks remain. Focused units and ten actual non-graphical
cross-role/streaming methods passed. Four retained graphical traces independently
satisfy the corrected predicate and reject injected identity leakage; those
replays do not replace fresh hosted graphical process execution.

Adversarial relay fixtures also lacked the newly required identity/knowledge
fields, causing decode rejection before the intended duplicate, invalid quantity
or retained-state-size boundary. Their valid premises now include explicit
unknown metadata while the original corruptions and byte bounds are unchanged.
Ten relay unit checks and three actual text-client repair cases passed. Separately,
the generated 3D fallback package omitted its new relic identity: eight native
methods failed the shared package validator before rendering. The exact package
helper now validates with an explicit cosmetic-fixture identity; all native
assertions remain for the next hosted run.

Further generated item fixtures needed explicit canonical identities; bounded
compiler checks reproduced and resolved thirteen failing authored copies without
changing mechanics. Fourteen focused runtime methods subsequently passed across
the repaired fixtures, including source-free state and privacy assertions. A
Rogue staff fixture's canonical-key seeded permutation now expects electrum rather
than nickel, verified by an independent SHA-domain/seed oracle and the actual
text/charge/restart process. Restoration's saved-knowledge oracle now uses the
canonical identity, while birth continues to use the archetype. Neither repair
changes the seed, effect or balance.

The earned-route acceptance fixture followed its established append-only format
transition: baseline v30 adds current-schema fields while v29 remains byte-exact.
Independent reverse-normalization checks preserve all non-schema authored values;
this is not a new tuning result. All three actual acceptance methods passed with
frozen source inputs, including exact source-free reexecution and the zero/one
command-budget cases. Previously unrun measurement slots remain unrun.

After the ASCII barrier was repaired, a broader local client check found one
additional sampled-item fixture in client-common carrying a true identity into
a disguised projection. The original acquisition, hydration, stale-memory and
non-authority assertions remain; only its disclosure metadata is corrected.
The independent place benchmark also completed its actual workload and exact
save recovery after its version-oracle repair. Fresh hosted qualification of the
coherent follow-up remains required; the first failed matrices are not passes.

The repaired client checkpoints passed all 178 ASCII integration methods, 188
client-common unit methods and 103 client-common integration methods, plus 300
text/headless methods. An intermediate isolated client-common invocation omitted
its required `test-support` feature and failed compilation; that command is a
harness configuration error, not regression evidence. The correctly configured
owner passed without changing feature gates or production code.

The next Linux release Rust matrix passed the backend and simulation owners, then
found the shared performance trace's remaining version-1 expectation. Unlike the
independent place fixture above, this trace intentionally reads the migrated
item geometry fixture v2. Its test now independently expects 2 while preserving
all accepted outcome labels and the misspelled-outcome rejection. This is a
separate stale version oracle, not a performance or gameplay change.
The focused shared-support owner then passed all 26 tests with the corrected
version oracle; formatting and documentation checks also passed.

A later Linux debug process shard exposed one independent driver race in the
owned-hit-die paid-preparation test: `wizard arena step` acknowledged the step
budget before the AI executed its action. An immediate Observe on another socket
returned revision 2 without preparation, while the retained text stream then
reported Spark Lance starting with 72 ticks remaining. The driver now waits for
the committed ability-start event and settles its visual projection; it does not
wait for the expected reserved-mana value. Exact payment, reservation, ability,
unrelated-hit-die preservation, owning-hit-die cancellation and restart/rewind
assertions remain unchanged. A deterministic ordered-frame regression failed
with the original immediate-Observe behavior and passes with the event barrier;
the two hit-die process cases and adjacent paid-preparation restart case also
passed. The unchanged original passed locally, so this is a CI-evidenced race
with a deterministic driver regression, not a claim of a locally reproduced
scheduler failure or a gameplay defect.

The subsequent CI run exposed an omission in that repair's affected-test
selection: the separate wizard-socket ordering driver called the changed helper
but still expected its removed initial Observe request. Both ordering-test
methods failed (four ability/order subcases plus the rejected-sync case), not
five independent defects. The local first-red reproduced those exact stale fake
expectations. The fake now exercises the real ability-start barrier, rejecting
early Ready and old snapshot frames before accepting the event; observer updates
still cannot refresh the Text socket. Both socket orderings, all three explicit
sync-before-removal boundaries, rejected-sync termination without mutation or
retry, and exact remaining snapshot requests are retained. A repository-wide
caller search identified both driver owners; all six affected unit/process
methods passed together after adaptation. No production behavior changed.

## Exact-change pose publication, 2 November 2026

A bounded renderer optimization exposed redundant Bevy change notifications:
repeating the same explicit pose on a real articulated Human produced identical
joint values but marked all 16 joint transforms changed. Two primitive publication
checks and one actual-rig check failed at that boundary before the fix; these are
three deliberate optimization regressions, not three newly discovered gameplay
bugs. Exact equality now gates publication without skipping pose/contact work,
advancing time differently, suppressing tiny changes or changing composition scale.

The extended actual-rig witness covers five independent humanoid outfits, a native
robe, Emu, Quagga and Centaur: 146 joint publications leave zero transforms dirty
when values are unchanged. Positive controls still publish a changed pose and
1e-8 translation/rotation. The renderer library passes (611 tests; its existing
subprocess-only fixture remains exercised by its owning lifecycle test). Five
same-adapter software-rendered wardrobe-motion captures, including walking,
jumping and armor replacement, retain identical decoded pixels. This establishes
avoided writes and visual preservation, not an FPS, hardware-GPU or release-time
speedup; no separate release comparison was run under the VM's disk constraint.


### Independent humanoid walking carriage (2026-11-02)

The shared animation owner now exposes optional Native, Grounded and Poised
carriage independently of body, outfit and gameplay identity. Before implementation,
two intended feature witnesses failed: explicit profiles resolved to the same
coefficients and produced the same hip pose. Native coefficient parity and strict
review-schema controls were already green. An initial new contact fixture also
failed on unchanged Native because it applied the straight-path sliding-share
bound to an abrupt turn; that fixture was corrected to the existing canonical
straight/curved/stop paths and their respective bounds, not counted as a production
regression. Existing contact assertions were not relaxed.

Six focused contracts cover Native parity, sex/body-independent explicit profiles,
actual joint/stance differences without clock or arm-cadence changes, strict
selection and teleport retention, all 32 adult-female wardrobe selections, and
real planted-foot movement across five body/outfit combinations on three paths.
Specialized non-humanoid/Guardian/Yeti/Zombie locomotion rejects explicit review
profiles. Grounded and Poised steer future landings without relocating planted
feet; defaults retain their historical path.

The full renderer library passes 617 tests (one existing lifecycle-owned ignored
fixture), and Clippy passes with warnings denied. The built renderer completed
five Native wardrobe-motion captures with identical decoded pixels against the
frozen pose-publication baseline. Fifteen additional matched-body comparison
captures cover Ranger, Knight and Sylvan Duelist at rest, walking from two angles,
turning and settling. Front/three-quarter walking and Ranger turning were visually
inspected; the real contact tests, not screenshots or generated concept art,
establish planted-foot behavior. No outfit geometry, network/gameplay contract,
mesh-cache identity or automatic sex-based profile selection was added. Native
remains the live/default choice; these optional profiles are reusable art controls,
not a claim that every character now uses a different gait. Hosted qualification
and hardware performance measurements remain separate from this local checkpoint.

### Ordinary skirt pelvis clearance (2026-11-02)

Actual Courtier, Tavern and Juliet captures showed an underlayer-colored oval
through cloth immediately below the waist. The first-red actual-mesh regression
measured 0.02284 model units of pelvis penetration for a slender Courtier. It
samples emitted cloth triangle vertices and interiors against the pelvis support,
not image colors or the fitter's own cached offsets. Coverage now spans those
three outfits, both sexes, all three builds and all three figure choices (54
compositions), with a nonempty geometric witness requirement.

The shared skirt builder now applies the existing pelvis-fit stage to ordinary
skirts too. Its first draft correctly cleared the pelvis but failed the existing
Medusa waist stand-off limit; that assertion was preserved. The final fitter keeps
ordinary attachment rings exact and fits the first free triangle interior as well
as its lower ring. This avoids both widening the waistband and leaving a small
near-seam intersection. Authored hems, splits, materials and body dimensions are
unchanged. Raised/paired waist fitting retains its existing path.

Seven focused skirt contracts pass, including existing thigh/waist, embroidery,
layer-order and cloth-motion witnesses. The final full renderer library passes
618 tests (one existing lifecycle-owned ignored fixture), Clippy passes, and the
actual renderer builds. Matched front/low/high captures remove the static oval in
all three affected outfits without changing necklines or inflating the skirt hem.
Walking/turning/settling captures were retained before and after as well: they
expose a separate pre-existing moving-thigh intersection in ordinary rigid-sway
skirts, which this static fit does not claim to repair. That dynamic cloth/contact
gap remains follow-up work; passing sway/layer tests is not proof of dynamic leg
clearance. Hosted CI qualification is still required before merge.

### Rogue birth-inventory authoring and retained baseline rules identity

The pinned `scripts/fixtures/rogue-birth-item-reference.json` independently records
26 species carry chances and 79 subtype weights across seven categories from
Rogue 5.4.4. Content acceptance checks both Rogue packages; they do not substitute
for runtime birth, ownership, replay or AI qualification. Existing floor policies
remain unchanged apart from the explicitly approved dagger quantities and arrow
equipment-generation recipe. The floor safety-food stage remains a separate TOR
adaptation, not a claim of exact source-wide food-counter parity.

Baseline v31 is an append-only source snapshot of v30 with only the
rules import changed from identity 57 to `interactions-v58`. Its receipt
retains the previous source/dependency hashes. Tests reverse only this import and
the bundled rules identity, then compare every remaining byte to v30; no roster,
stock, quantity or numerical policy is refreshed. Historical v30 and earlier
source/result receipts remain untouched. The current process selector uses v31,
but these source tests do **not** requalify historical outcomes or establish a new
successful route. Compiler validation and ordinary-process qualification remain
separate evidence.

### Rogue carrier generation, shared equip-one and observer-safe item care

The existing carried-pool representation now accepts up to 128 entries independently
of the unchanged 64-entry population-band bound. Both actual Rogue packages
compile 79 canonical subtypes and round-trip through source-free compiled content.
Exact category/subtype weights and all species carry policies are checked against
an independent pinned reference, not probabilistic sampling. Loot-entry quantities
remain the sole authoring mechanism. The generated-arrow alias preserves mundane
starter/trap ammunition; only generated loot receives its condition recipe.

Carrier food history reconstructs committed ground/carried food generation from
immutable activation receipts, including retired food. Unit and actual activation
witnesses distinguish initial actor births before the floor increment from later
room-cluster items/actors, skipped generation, policy changes, absent food choices,
and unchanged carry chance. Existing ground-food safety and all unrelated floor
placement laws remain untouched. These adaptations do not claim an identical
Rogue RNG stream or a single source-wide food counter.

The equip-one change reuses ordinary exact-unit reservations and transactional
extraction; it does not grant throwing or permit modifier/launcher stacks.
Actual processes show an eligible passive actor transition from no equipment to
one equipped member of its three-unit birth stack, preserve the two-unit remainder
through source-deleted restart, and drop all three units on death. The same AI
profile without compatible anatomy leaves the full stack unequipped. Independent
known-item care uses shared effect safety and normal action admission; unsupported
or unidentified items remain carried rather than acquiring hidden knowledge.

Full server and actual carrier restart qualification exposed a real omitted
journal gate: captured exact-unit Equip operands were absent from the persisted
human-admission action allowlist. The correction admits that existing legitimate
action without weakening operand shape checks or changing the archive schema.
A queued armor-Equip regression checks the exact captured unit, serialization,
checkpoint before execution, and rejection when the same binding is attached to
Wait or Unequip. The original server/source-free restart failures are retained as
integration first-red evidence. Tests that saved only after execution had missed
this boundary; the qualification did not classify the failure as stale output.

Separate fixture repairs isolate newly legitimate magical birth-stock roots in
the existing source-reachability helper, retaining its explicit negative/positive
capability witnesses and the full production catalog. The authoring stack test now
accepts a homogeneous melee-only stack while retaining currency, launcher,
modifier, potency and relocation rejection cases. No production reachability
check was relaxed.

Local final-source qualification passed 2,353 simulation tests, 1,301 server
unit tests, eight selected integration boundaries, three encoded Service
contracts, and seven real server/client process methods. The process set includes
known-versus-concealed born medicine and formerly failing centaur/yeti source-free
restart. Clippy, changed-file formatting and pinned Python lint passed. All 47
current scenario packages validated, and the current archive fixture was produced
by the real writer without an archive-format change. Distribution/authoring,
retained-baseline, documentation and CI-owner registration checks remain separate
source/tooling evidence. The full platform/native matrix and extended campaign
suite are deferred to required final-head CI, not represented by these local
checks. Disk-exhausted build attempts were retained and retried after bounded
obsolete-cache cleanup; they were not gameplay failures or waived tests.

### Rogue birth inventory CI fixture admission and receipt follow-through

The first hosted full run found one stale simulation integration fixture pattern
across three methods: successful Equip calls bypassed ordinary intention admission
and therefore lacked the exact-unit reservation now required by shared equip-one.
Those methods now use the existing admission/execution path, retaining hostility
pause/resume, authored timing and custom-slot checkpoint assertions. The adjacent
overflow and capacity negatives also enter through admission; overflow pins the
actual TimeExhausted error rather than passing for a missing reservation. The
initial local checkpoint had covered simulation unit tests and selected server
integration, but missed this simulation integration owner.

Hosted process checks also exposed current-source fixture drift: the shared
equipment-receipt helper still expected the mundane arrow instead of the generated
alias; a production carrier assertion still expected historical ration/bandage
stock; an inverted-range mutation targeted a row that no longer existed; and a
throwing test still expected multi-unit Equip rejection. These are distinct
fixture causes, not additional gameplay defects or platform-multiplied counts.
Historical baseline bytes and candidate oracles remain separate from current
production expectations.

The repaired checkpoint passed all 815 simulation integration methods and their
Clippy target, 12 selected real-process methods, the directly affected helper and
source-unit suites, pinned Python lint and documentation checks. The process
selection includes the original exact thrown-payload/source-deleted restart and
full earned-second-HD, finite-stock and floor-revisit witness; a separate new
process checks accepted Equip-one with conserved remainder and source-free
restart. The corrected range mutation asserts that it changes exactly one row
before testing rejection. No production code, historical baseline or verification
gate changed in this repair. A disk-exhausted local test build was stopped and
retained as infrastructure evidence, then the complete integration target passed
after bounded obsolete-cache cleanup. Fresh final-head CI remains required.

## Save-owned client captures and report notes, 3 November 2026

During development, review found that the advertised 8 MiB PNG limit did not
fit the previous 8 MiB default save queue once its journal envelope was included.
The independent exact-size PNG fixture passed production image validation, then
`storage::attachments::tests::maximum_png_payload_is_admitted_flushed_and_reopened_with_default_queue`
failed at ordinary default-queue admission with `StorageFailure`. This is one
development production root, one distinct failing method and one failed execution,
not a released-data corruption claim. Raising the bounded default to 16 MiB made
the same admission, unchanged game state, durable flush and byte-identical reopen
witness pass. Explicitly smaller policies still reject oversized records atomically.

A separate development UI defect consumed and closed a 3D Note/Report draft
before a busy screenshot/readback or upload slot rejected Enter. Two distinct
regression methods each failed at the preserved-text assertion before the fix
(one root, two methods, two failed executions; audience/report subcases do not
multiply those counts). The narrow editor guard preserves text, mode, audience
and snapshot while blocked. Positive release/retry controls still send the exact
ordinary annotation once and close the editor afterward.

Dedicated server witnesses cover immutable private bytes and snapshot anchors,
exact retry and interrupted/expired uploads, spectator denial, global staging and
per-save totals, small response-frame retrieval, journal/blob transaction rollback,
checkpoint parity and source-free archive copies. Adjacent ordinary-note, receipt,
queue and checkpoint-retry regressions pass without changing annotation semantics.
The real Text client saves its exact emitted UTF-8 tail, preserves earlier captures,
prepends `PLAY TEST REPORT` through ordinary annotations, and reopens after deleting
the scenario source; its spectator case rejects both capture and report submission.

The first common-client library run preceded the mandatory real wire recording:
one missing-fixture setup cause affected 23 methods/executions, while 167 passed.
Recording the complete protocol 69 fixture through real server/client processes,
including independent returned attachment metadata and decoded-byte equality,
resolved those setup failures; all 190 common library methods then passed. The
fixture is not a mechanically relabelled older recording. One documentation method
also caught four stale protocol references; current-identity links repaired that
single documentation cause. A wider server-unit pass additionally exposed six
setup failures from the missing save-67 scenario fixture; recording it with the
actual current writer and independently matching that writer repaired this second
fixture-setup cause. Three methods across protocol and server retained explicit
old version pins after the approved transition. These are stale test expectations,
not compatibility regressions; the assertions remain explicit at 69/67. The
first wider server pass had 1,313 passing methods and only these eight setup/pin
failures. Deliberate report/3D feature first-red checks are not additional
production-defect counts.

The standalone common integration invocation also initially omitted its required
`test-support` feature and stopped at compilation. Correcting the invocation ran
all 108 methods successfully; that setup error is not a failing behavior method.

Disk-exhausted compilation attempts are infrastructure failures, not failed
behavior assertions. Their original results were retained and retried only after
bounded, owner-coordinated regenerable-cache cleanup. The approved isolated Xvfb
packages could not start in this environment: Unix-socket creation was blocked,
and the supported escalated launcher failed before executing the display server.
No network/display-policy workaround was used. Native ASCII/3D window capture
acceptance remains required hosted-platform coverage, not established by unit PNG
or offscreen rendering tests. Local checks do not replace final-head CI.

Final local qualification passed all 1,321 server unit methods, 79 protocol unit
and 299 integration methods, 190 common-client unit and 108 integration methods,
180 ASCII integration methods plus six model and six capture/PNG unit methods,
and six selected Text report methods plus three exact-output methods. The 3D
checkpoint passed 43 affected renderer methods and 77 model methods, including
the two draft-retention repairs, with all-target Clippy. These totals include the
focused witnesses above rather than adding repeated executions to them. Two real
Text capture processes and eleven combined-runtime Rogue process regressions
passed; the longer Rogue playable run retains its separate owner qualification.
The complete wire recording contains 77 actual server sample kinds, not 77 test
methods. Required final-head CI owns the wider integration, release and native
window matrix; none of the local results claims that matrix passed.


A follow-up review found one recoverable client-side capture cleanup defect:
a known commit rejection left the connection's staged upload until its 60-second
expiry. The explicit rejected-commit witness first received a fresh Begin instead
of the required exact-ID Abort, failing that behavioral assertion. A second
cleanup-error witness timed out because no Abort was sent; it is supporting
evidence for the same cause, not a second defect. The client now preserves the
confirmed rejection through synchronized request completion, best-effort aborts
only that staging identity, and returns the original error. Server commit,
privacy, persistence and idempotency semantics are unchanged. Four new scripted
wire methods cover fresh capture after cleanup, cleanup-error preservation,
disconnect and malformed-response unknown outcomes; all 112 common integration
methods pass. No uncertain commit is retried or interpreted as a known failure.

The generated protocol-69 recording was subsequently split into bounded,
content-addressed JSON parts for publication. Independent Python and Rust
reconstruction both matched every original value and ordering: 67 client samples
and 77 server samples, with no new or omitted samples. The manifest references
21 parts, each at most 100,000 bytes. The recorder writes immutable parts before
atomically replacing the manifest; failure-path tests preserve the prior fixture.
The shared test/example-only Rust reader validates protocol, part ordering,
counts, paths, digests and byte limits. Its intended first-red failed at the
reconstructed-value assertion, then all six helper tests passed; 77 recorder
methods and pinned Ruff checks passed. An earlier full protocol test compile
failed with disk exhaustion before running tests and is not behavioral evidence.
Check-only consumer qualification is recorded separately from normal Cargo test
execution, which remains a final-head CI gate. This representation change creates
a new tree; prior exact-tree qualification is not claimed for changed fixture
readers. Review additionally found four server/ASCII reader sites outside the
initial protocol/common edit scope; they were included before publication rather
than leaving their old whole-file assumptions behind.

## Screenshot HUD expectation maintenance, 3 November 2026

PR 224's hosted Linux 3D debug run exposed a **stale test/oracle**, not a
production defect: `test_client_process.Native3DProcesses.test_framed_hud_dock_opens_inventory_without_a_world_action`
expected the seven pre-capture dock actions and rejected the newly supported
`Screenshot` action. The assertion remains an exact set; its expectation now
includes Screenshot. The related synthetic dock fixture in
`test_verification.NativeMouseContract.test_hud_background_target_remains_reachable_on_the_default_xvfb_desktop`
needed the same update. After correcting the native expectation, that focused
fixture failed specifically because Screenshot was missing, then passed after
its action list was corrected. This bounded evidence window has one stale
expectation root, two distinct failing methods and two failed executions (one
hosted native failure and one deliberate local fixture-maintenance check).
The deliberate local check is not an additional naturally detected defect.

All 34 pure Python 3D verification methods pass after both updates. Native
window/OS-input execution was not repeated locally; hosted final-head execution
must qualify that boundary. Existing geometry, no-world-action and exact-action
assertions are preserved. No Rust or production behavior changed.

## Throwing disconnect picture barrier, 3 November 2026

PR 224's Python process shard exposed one stale-picture fixture race in
`RogueThrowingProcesses.test_text_carried_dart_and_shuriken_preserve_exact_pending_payload_after_source_deletion`.
The retained probe stream publishes a visual-changing outcome at sequence 16,
then the paused intention at 17 while the observation still says preparation is
active, and finally the observation delta at 19 with preparation inactive.
The saved pending action and restored observation were correctly paused, with
43 preparation ticks remaining. The fixture compared the intermediate causal
frame's old picture against that restored state.

The pause barrier now uses the existing `picture_after` helper after observing
the paused intention. It does not advance the world or relax state equality,
intentions, frozen plan, item holds, RNG, or source-free restart assertions.
A deterministic ordered-stream regression fails against the old barrier in two
subcases: pause arrives before its picture, and pause is already seen while the
picture is still dirty. Both pass with the shared picture barrier. This is one
fixture-race root, one naturally failing hosted method, and one deliberate unit
regression with two failing subcases, not three production defects. The original
strict real-process witness remains a final-head hosted CI gate; protected local
process binaries predate later capture changes and are not claimed as current
runtime qualification.

## Linux storage probe columns, 4 November 2026

CH-013 is one production parsing defect: whitespace splitting treated a model's
first word as the transport when `lsblk` returned an empty middle `TRAN` column.
The isolated `MachineProbe.test_linux_probe_blank_transport_preserves_model`
regression failed against the original probe for both single-word and multiword
models, then passed with explicit JSON columns. These are two subcases of one
method and one root cause, not two defects. The fixture executes the actual
probe with subprocesses, machine-file reads, CPU count and kernel identity mocked;
it does not establish prevalence on real hosts.

The complete performance-ledger owner passed 26 methods under stdlib Python
3.12.14. Controls retain exact previous fingerprints for present transport,
missing model and empty device output; other cases cover JSON rotational
encodings, NVMe, quoted models, existing parent selection, command failures and
malformed output. Corrected metadata changes affected fingerprints prospectively;
historical measurements and fingerprint construction remain unchanged and must
not be rewritten to combine old/new timing series. Windows probing is unchanged.
No Rust builds, actual-device probes or broad verification tiers ran. A qualified
pinned Python environment, lint, independent review and final-head hosted CI
remain required before publication/merge qualification.

The first hosted Windows debug Python shard then exposed one fixture portability
root across all eight new probe methods: Windows rendered the fixture path as
`\fixture`, while the mocked command table expected `/fixture`. Its mocked file
lookup likewise assumed POSIX separators. This is a test-fixture defect, not a
second production storage-parser defect. Expected command paths now use the
host's path rendering and mocked file lookup uses normalized POSIX keys. A
`PureWindowsPath` fixture regression reproduces the exact command-table `KeyError`
on Linux before the correction and passes afterward; all 27 owner methods pass
locally. Hosted native Windows verification remains required, not replaced by
this path emulation. The failing evidence is
[PR 255's first Windows debug shard 2](https://github.com/KCall360/thresholds-of-ruin/actions/runs/41823447489/job/126561124600).

## Staff disconnect picture barrier, 4 November 2026

PR 258's Linux release Python run exposed the same stale-picture fixture-race
class previously repaired in the throwing owner, now in
`RogueStavesProcesses.test_text_staff_physical_hit_charge_exhaustion_and_pending_source_free_reopen`.
The retained probe stream has a visual-changing outcome at sequence 48, paused
intention at 49, and the demanded observation delta at 51. The intermediate
picture still has active preparation and is not ready. The delta's disclosed
state exactly matches the reopened state (excluding only the existing local
picture identifiers), and the intentions match in both frames. Persistence is
not implicated by this evidence.

The staff pause barrier now consumes the demanded picture through the existing
`picture_after` helper before retaining its restart oracle. All strict state,
intention, frozen plan/payment, item and RNG comparisons remain unchanged.
The deterministic ordered-stream regression first failed at its picture equality
assertion in both timing subcases, then passed with the barrier; all 128 affected
fixture and process-harness unit methods passed. This is one additional naturally
failing process method in the already documented fixture-race class, not a new
production defect. The two deliberate unit subcases are sensitivity evidence,
not additional naturally detected failures. The actual Text/source-free restart
witness remains required in final-head hosted CI; no local current-binary process
qualification is claimed.


## Retention diagnostic root limit, 5 November 2026

PR 266's first Linux release Rust run exposed one diagnostic-fixture root cause
in all four eight-owner retention methods. Their 384 distinct roots exceeded
the production checkpoint codec's 257-root capture bound; serialization rejected
the invalid capture. The four single-owner methods passed. This is not a
production retention defect or missing restore binding. Evidence: [Linux release
job](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42100439774/job/127504306775),
artifact 13452188782 (SHA-256
`cfbd4e89ba5b99405b80ad59df622532e0757de99489c2fa1bf8f9308d192113`).

Diagnostic serialization now uses legal independent batches, reports their summed
JSON bytes and count explicitly, and asserts two batches for eight owners. It
preserves the production bound, all ownership/reclamation assertions, and normal
checkpoint recovery. Cross-batch sharing is not counted by this size proxy.
Final-head hosted execution remains required; no local behavioral run or isolated
RSS/timing result is claimed while local storage capacity is constrained.

## Menagerie checkpoint capacity, 6 November 2026

The supplied extended-play incident exposed two related persistence weaknesses:
whole actor-table and AI-state repetition multiplied large records across the
bounded rewind window, and optional checkpoint capacity failure stopped legal
journal batches from becoming durable. The private deployed save was not copied
into the repository or used as a local fixture; the reported 64 MiB cap incident
is distinct from the reduced-limit regression below.

Three deliberate regression methods reproduced the intended pre-fix behavior.
`storage::checkpoint_failure_tests::capacity_deferral_keeps_journal_durable_with_and_without_prior_checkpoint`
failed at explicit journal flush after a real encoder overflow. The simulation
`checkpoint::tests::changing_one_actor_does_not_repeat_other_actor_records`
validated all restored states and mutation isolation, then measured 2,308,224
bytes for 64 boundaries of 32 actors against 36,787 bytes for one boundary.
`checkpoint::tests::retained_ai_records_share_visits_and_restore_independent_mutable_states`
validated restoration/isolation, then found 96 serialized visit maps where four
complete AI records suffice. These are three sensitivity witnesses, not three
independent naturally detected production defects or platform-multiplied counts.

Save-format pooling requires current-format fixture updates. Test-only mutable
actor access preserves the former per-directory mutation boundary through
copy-on-write; malformed raw references remain raw tests. Historical corruption
must not silently corrupt a current shared payload and thereby weaken its
historical-only rejection witness. Verification outcomes are recorded with the
change; local focused checks do not replace final-head hosted qualification.

The same synthetic actor workload now encodes to 160,170 bytes (93.06% below
its 2,308,224-byte baseline); its single snapshot is 36,907 bytes. This is a
serialized-size witness, not a release latency benchmark or a measurement of the
private Menagerie save. An additional restoration-cache witness first failed at
rebuilding an unchanged actor across distinct directories, then passed alongside
item-table scoping and mutation isolation. A deliberately failing generic
serializer also exposed an intermediate capture panic before the guarded hash
sink was added; this was an implementation-stage regression, not a separately
observed gameplay incident. Oversized fingerprinting and other serializer errors
now have distinct handling, with reader and writer expansion checks kept symmetric.

Broader checkpoint checks also detected stale multi-root fixture API usage:
full save restoration and boundary equality succeeded, but a later independent
single-root restore correctly rejected unused records from the multi-root pool.
Those fixture reads now bind the complete root set before restoring individual
snapshots, retaining exact equality and corruption-rejection assertions. Three
independent weapon round-trips now start a fresh pool after changing state rather
than carrying unreferenced records from the preceding round-trip. A new streaming
fallback fixture initially assumed that entering a hall implied detachment;
perception and physical-neighbor pins invalidate that assumption. Its setup must
prove actual movement and detached references before testing durable row retention.
These are fixture corrections, not additional observed production incidents.
The streaming fixture's public position comparison was also observer-relative
zero; it now verifies authoritative movement-event coordinates and the authored
start anchor. A subsequent real execution reached valid detached rows but exposed
a missing NPC-turn driver in the new fixture's continuation, rather than a
persistence failure. The fixture uses the existing bounded ordinary AI progression
pattern and keeps unexpected movement failures fatal.
That corridor had already saved all seven authored source rows before fallback,
so expecting its source count to grow was an invalid setup oracle. The streaming
witness instead preserves those exact saved sources and reopens without the
original package; newly admitted source/generated-row atomicity is independently
asserted by the transaction-boundary regression. No production source-admission
requirement is removed.

After these fixture corrections, the focused checkpoint set passed 55 simulation
and 87 server tests; the four additionally affected simulation fixture modules
passed 169 tests. Ten actual-process checkpoint tests passed with freshly built
server, Text and headless clients, including reduced encoder capacity with and
without an older checkpoint, continued durable play, restart, old-format rejection,
and a normal-limit 27-actor retention-window workload. These are local focused
receipts, not hosted or native graphical qualification. The private deployed
Menagerie scenario was unavailable and is not represented as verified.
The subsequent targeted integration phase passed 23 server persistence/streaming
checks and 40 simulation nutrition checks; nine actual-process background-save
checks also passed. These results include the source-free streaming fallback
witness and retain the same local-versus-hosted qualification distinction.

Release comparisons found smaller checkpoints and faster restart but repeated
interaction-latency increases, so a further capture-work regression was added.
After exact restoration and canonical-byte checks, the intended first-red
witnesses measured 33 actor fingerprint calls versus five required and 63 AI
calls versus seven required. A separate equal-allocation/cleared-memo witness
measured 15 calls versus five. Initial test setup incorrectly expected a revisited
AI value to create another unique record and attempted unsupported capture into a
decoded pool; those fixture errors were corrected without relaxing either
structural deduplication or decoded spatial-authority rules. Reduced fingerprint
work alone is not evidence that the observed latency difference is resolved.
The memoized run also exposed one old fixture mutating an internal canonical
record after capture, leaving its fingerprint metadata stale. It now captures an
invalid source clone and the valid original separately, retaining invalid-body
rejection and exact valid restoration without mutable access to pooled records.

The first exhaustive PR 272 run exposed 18 server fixture failures and one real
profiling-consumer integration defect. Seven server failures came from missing
current-writer scenario goldens and an obsolete literal save version. Eleven
came from optional actor/AI absence wrappers, obsolete pooled-record paths and a
single-root restore of a multi-root hallucination checkpoint. Fixes preserve
optional absence only in the named wrappers, strict malformed-reference handling,
COW negative mutations and complete-root restoration. The golden is recorded
from the current writer; historical fixtures remain unchanged. The profiling
validator had treated the new nullable deferral sequence as a mandatory numeric
count. Its regression preserves missing/null/value distinctions and still rejects
invalid values and null ordinary counts. These failures are distinct from the
passing focused checks and are not evidence of exhaustive qualification.
The correction passed all 1,348 default-feature server unit tests, the exact
previously failing streaming-benchmark process test and 67 report-validator tests;
three focused validator tests also passed under Python -O. Hosted CI enables four
additional service-contract-host tests, all green on the first head but not
included in that local count. The regenerated version-68 scenario golden is
byte-identical to the preserved version-67 metadata fixture because the independent
scenario metadata formats did not change. Recording mode was unset for the normal
writer comparison, round-trip and malformed-schema checks.

Later lanes in the same exhaustive run exposed two nutrition process wrappers
that still treated pooled actor indices as full records. The wrapper now expands
complete actor and AI values with independent copies for each binding and retained
root. Its first-red alias/expansion regression and 15 helper checks pass, as do
both previously failing real-process nutrition cases using the unchanged current
production binaries. These are test-consumer corrections, not a relaxation of
checkpoint validation or the continuation comparisons.

The Windows debug native travel failure is independently explained by retained
frames: a distinct return journey had arrived with one step and admission ready,
but its picture was still marked not current at tick 100. The mouse path completed;
the fixture consumed status before presentation. Native travel now uses the same
arrival/admission/current-picture barrier as automated travel. Scripted regressions
replay the real test body and frame reader, retaining exact input ordering, distinct
journey identity and tick 100/200 assertions. The old predicate first failed at the
original tick assertion for delayed pictures and at the admission witness for
delayed permission. Mocked OS I/O is not native qualification: the actual native
Windows/Linux lanes remain required, with no waiver or relaxed assertion.
The final combined local helper, selection and reporting run passed 77 tests;
20 documentation checks and whole-scripts Ruff also passed. The registry keeps
the new scripted class non-graphical and the original native case graphical;
existing unregistered owners still require exhaustive fallback rather than being
silently accepted by selective coverage.
