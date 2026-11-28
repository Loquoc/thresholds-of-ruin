# Test-effectiveness ledger

Keep a small, evidence-backed record of what tests actually catch and what
maintenance they cost. This is not a pass-count leaderboard, a new merge gate or
a requirement to audit every run. The [testing policy](testing.md) owns coverage,
execution cadence and qualification; [test objectives](test-objectives.md) owns
coverage intent. This page retains lessons that help maintain those tests.

## Maintaining this record

At a coherent fix checkpoint, consolidate evidenced defects and stale-test
findings into reusable lessons: the violated contract, the independent witness,
the correction (or
unresolved limitation), and what the evidence does **not** prove. Update an
existing lesson rather than appending another execution narrative. Preserve
meaningful negative results, rejected approaches and known coverage gaps.

Keep routine pass totals, command transcripts, interrupted-build chronology and
per-platform reruns in the PR's verification summary or CI artifacts, not here.
A PR summary should identify the tested revision, selected/reused coverage,
failures and checks not run. Do not link machine-local logs as public evidence.
A concise finding here complements that evidence; it never replaces required
regression tests or final-head CI.

This is a curated record, not a repository-lifetime defect census or a live
qualification dashboard. Historical detailed entries remain in Git history
before this curation (the pre-effects ledger is present at main commit
`5d2099af`). Removing execution narratives from this page does not mark open
defects fixed, turn local results into hosted CI,
or qualify a previously untested platform or pose. Current unresolved work
belongs in the relevant feature guide or [roadmap](../status.md).

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

## Explicit status and ordinary narration

When suppressing routine preparation and recovery feedback, keep one full text
Adventure test that exercises explicit status, self-examination, and an ordinary
room description separately. The integrated suite caught an obsolete expectation
that explicit diagnosis would never show a recovery count, although the new
contract requires it. Updating that expectation did not relax the controls that
keep timing out of ambient description and self-examination.

## Disclosure and observation boundaries

### Event-time sighting absence, 5 November 2026

Published baseline `5c87b5719` measured time since arrival instead of time out of
sight. Two independent ASCII witnesses caught one production root:
`event_time_sightings_measure_absence_from_latest_departure` and
`model_outcome_sightings_use_departure_time_without_refreshing_the_picture`.
The latter applies real `OutcomeFacts` without refreshing the visual snapshot.
Recording each disclosed departure tick fixes the root; repeated-cycle, reset,
identity and silence assertions prevent a first-encounter-only fix. These were
local debug witnesses, not native-process or cross-platform qualification.

### Authoritative effect disclosure and observed lifecycle

Existing magic-item privacy and unpublished-observer tests exposed two
production defects during development: non-source observers received spatial
effects before their ordinary observation commit, and restricted/unknown device
sources leaked events through anonymous geometry. Delivery now requires both
ordinary observation proof and the event-time source license. Keep those
assertions independent of rendering. A fixture that omits its observation
boundary must establish that boundary, not weaken the publication guard.

Synthetic populated public-effect snapshots exercise protocol/client lifecycle
contracts. At this checkpoint the live server emits an empty persistent-effect
list: synthetic start/update/end, cloud, cone and burst witnesses do not prove
live lingering-cloud gameplay or world expiration. Retain that distinction when
extending producers and clients. Positive headless fixtures must use protocol
position ordering and licensed appearance cues; correctly rejected unsorted
cells or anonymous elemental cues are fixture errors, not failed delivery.
Invalid anchors and timing remain deliberate negative cases.

The ASCII regression review found ambiguous repeated portal cell keys and ray
orientation retained from an old frame. Rendering now omits ambiguous keys and
uses directional glyphs only between uniquely mapped adjacent cells in the
current frame; missing or disconnected neighbors use a neutral marker. Keep
regressions for duplicate keys, ordering, rotation and lost neighbors. This is
a presentation defect family, distinct from the server-disclosure defects.

Desktop playtesting exposed generic flash narration on ordinary melee and
Search, separate staff preparation/completion messages, and a short-range ray
that disappeared before the player could recognize it. Actual-process
regressions reproduced the unwanted flash and redundant zap messages before
the fixes. Keep wording expectations current in both client suites and retain
their authority and privacy assertions. Keep model tests for ray progression
and its terminal hold. Live recordings measure how
long the glyphs remain visible after delivery; server event timing alone cannot
establish that presentation interval.
History and disclosure tests use a non-basic ability with a visible target
effect to retain their geometry and authority witnesses. The condition test
checks the remaining melee ability receipt, including actor, target and outcome.

The same review reproduced a diagonal doorway attack refused beside one wall,
plus an out-of-reach refusal that omitted repositioning guidance. Horizontal
diagonal melee now shares movement corner clearance. Keep the doorway process
witness and cached-contact tests that move a blocking body into and out of the
remaining open side. Verify both attack admission and the informative refusal,
since a correct reach predicate alone does not establish the player's feedback.

## Resource ownership and asynchronous clients

### Bounded deferred ingress

`runner::stalled_ingress_tests` reproduced one deferred-ingress ownership root
against baseline `6fab91097`: exceeding the independent 256-request bound,
retaining disconnected work, and losing real-transport query liveness under a
stalled writer. The correction caps admission, reclaims disconnected work and
preserves surviving FIFO and accepted receipts. Keep the real transport witness
alongside in-memory bounds, retry-at-overflow and save-becomes-overdue cases.
These establish bounded request ownership, not total retained bytes or an RSS
improvement. The original [first-red job](https://github.com/KCall360/thresholds-of-ruin/actions/runs/42098728533/job/127495738606)
is historical evidence, not current-head qualification.

A recovery fixture also failed while establishing its accepted receipt, before
it reached overload. Establish that receipt before locking the writer and expose
setup output on failure. That fixture repair is not another production root.

### Observe the intended completion boundary

Cross-client projectile, disconnect-picture and earned-route fixtures exposed
why acknowledgement, admission, action completion and visible-picture commit
cannot be treated as interchangeable. Wait on the boundary whose contract is
being asserted; retain earlier-boundary tests when they intentionally exercise
paused, rejected or concurrent work. Increasing a timeout or waiting until an
unrelated later frame can hide the race the test should detect. The [testing guide](testing.md) describes supported drivers and lifecycle
boundaries.

For Rogue cohort reports, exercise intermediate phases before reporting a Search
outcome. The admission-floor fixture originally went straight to resolution or
failure. Adding a started and a queued observation made both counts fall to
zero. The parser now keeps each admission floor until a terminal phase arrives.
The fixture checks trace accounting across floors. It does not measure how
often this sequence occurs in campaigns.

## Procedural models: independent geometry and visible quality

### Test emitted surfaces, not only recipe parameters

Equipment development exposed corner-only clearance passes that missed cell
interiors, and attachment tests that inspected a joint path bypassed by cached
hand meshes. Use emitted faces and the actual scaled attachment chain. Retain
positive shape/width checks alongside clearance: flattening or inflating cloth
can avoid collisions while destroying its intended appearance. A visor oracle
at brow height missed covered eyes; test the actual anatomy across body variants.
These are lessons about test independence, not a claim that all outfits or
continuous motion have been visually qualified.

Native foot pigment isolation caught a false skin-colored boot shaft on bare
Troll/Nymph feet. Preserve both the negative shaft witness and positive foot,
calf-binding and equipped/restored controls across body variants. Multiple
figures or sampled poses are witnesses of one root, not separate bugs.

The bat and kestrel could finish a fall balanced on a buried wingtip while
their bodies stayed suspended. The grounded-fall witness samples real emitted
mesh vertices in both mirrored falls: torso contact at rest is independent of
minimum height across all body parts during the drop. The living-flight control
still excludes planted feet. These sampled poses and manually matched native
views are not a continuous collision or gameplay-flight guarantee.

The Medusa did not turn the front of her serpent crown toward her gaze target.
The witness samples distal emitted geometry in the head's frame at rest,
contact and release, so the actor merely leaning forward cannot pass. Keep
head aim separate from scalp attachment and crown-side checks. Native views
review the silhouette across poses and angles; the reach witness does not
prove continuous separation of every pair of serpents.

The grounded Emu could hold its head above the floor while its flank sank
through it. Check the emitted head and feathered torso separately in both fall
directions: rolling the neck fixed the head but left the flank below the floor
until the torso support changed too. A settled-pose check does not certify the
rest of the fall or every feather and toe.

Dragon and griffin tails could remain raised after the flank had settled.
Measure both the distal surface near the spade or tuft and the minimum over
the whole tail on both fall sides. This catches a raised tip even when the
root is within ground reach, without passing a fix that buries part of an
asymmetric tail. Native views cover sampled poses, not continuous contact.

Vampire and wraith used their fixed equipment fists to mime a grasp even
though their open free hands were already modeled. Measure reach and recall
of the same wrist relative to the torso so a body lunge cannot pass; use
native ability frames to verify the visible hand silhouette. This does not
assert combat hit timing or target contact on the server.

The Jabberwock's shoulder swing passed even when its claws faced up at
contact. Measure palm orientation through the actual forearm and wrist,
and judge forward reach and side separation in the actor's heading rather
than a fixed world direction. The native opening strike confirms the
changed silhouette, not the follow-up strike or combat resolution.

For multiple death motions, test the selection buckets independently from the
timing and visible poses. A long animation can pass its pose tests but still
disappear under the scene's old corpse deadline; the scene witness exercises
both transient bodies and those claimed by a disclosed corpse. Drive the
actual animation and cleanup together at ordinary and low frame rates:
elapsed time can outpace the capped pose clock. Distinct family motions
converge on the same settled poses, separate from server death.

The bat and kestrel formerly shared a talon-first lunge, although the bat's
authored fangs had no moving jaw. Witness the bat's hinge opening and
returning, and compare the whole native swing with the unchanged kestrel
control. The silhouette and pose tests do not assert that an attack lands.

The Xeroc ambush shut its jaw as the body reached forward, and the resulting
shell travel slid its short pads. Sample at presentation contact, not merely
at the windup: the advancing head must still gape, the supports must stay near
their rest placements, and the jaws should clamp during follow-through.
Matched native views separate this from the unchanged ordinary bite; they
do not test gameplay disguise release or combat contact.

The Adventurer's equipment tassets hung below the fitted belt even at rest,
and their hip-frame attachment opened a larger gap when the torso bowed.
Sample actual emitted waist-edge points in several front sectors and move the
witness into the posed belt frame; measuring against the hips can miss a
detached opening. Native comparisons preserve the lower green lining and
split, while separate thigh tests still own lower-panel clearance.

The Troll's scripted scalp scratch lifted a fist only to chest height.
Compare the emitted free-hand fingertips with the posed upper scalp across
different body builds, not the arm's angle alone. A shoulder lift can test
green while the gesture remains unreadable. The geometric reach check does
not establish clearance by itself; inspect matched approach, contact and
release from the front and the moving-hand side, including glove coverage.
Keep separate controls for headwear and active strikes.

The Flytrap's death pose kept its roots planted but put the lower lobe
through the ground. Measure both emitted lobes, including their teeth, along
the entire fall for both lean directions and all three death timings. Checking
only the head joint or the final root position misses visible penetration.
Require the settled rim to reach the floor, not merely avoid it, and keep the
living bite's reach and ordinary clearance independent of the death pose.

The Royal Bride's veil kept its crown attachment through the fall but
the terrain hid the trailing sheet. Measure weighted mesh vertices for the
cloth, border and settings in posed floor space rather than only checking
joint motion. A fixed tilt and lift cleared the ground but left too
much cloth hovering. Require some of the train to rest near the floor and
retain independent witnesses for the unchanged head anchor and shared mesh
with per-wearer skin joints. Native views are still needed to judge the drape.

### Retain failed optimization lessons

A contour-query shortcut reused the last crossing interval and found a different
support branch near an aperture. Comparison to the retained full-width reference
found a 0.754 mm displacement; the shortcut was rejected and the accepted fitter
restored. Keep the nine-profile, bilateral contour regression with its original
five-micrometre bound. This was not a requirement for pixel identity, nor evidence
of a speedup. Simpler code is not equivalent code unless the independent contract
holds across the parameter space.

Panel endpoint-row reuse reduced deterministic fitted-point evaluations while
paired timing remained inconclusive. Those counts are not SDF-query counts or
FPS. The earlier bounded panel-clearance correction incurred a measured local
construction-time tradeoff (about 9.81% median total overhead in that comparison);
subsequent query reuse did not establish that the cost disappeared. Keep exact
output, clipping, offset and supported/missing-transition witnesses, and report
construction cost separately from per-frame performance. See the
[performance policy](performance.md) for measurement provenance and comparison.

### Animation budgets during population changes

A cloak pacing review found that reducing both the scheduled slot and each wearer
ordinal modulo the previous population repeated a slot when new wearers arrived.
The first crowd frame and return from an empty scene could therefore fit every
cloak despite the one-wearer budget. Match the selected ordinal exactly instead.
Retain startup, growth, empty-return and multi-frame interval/population controls
alongside the stable-crowd fairness test. Extracted pure scheduler regressions
established the failure and corrected counts. The native Cargo owner run passed
all five scheduler tests, and the broader animation selection passed 26 tests.
These runs do not qualify visual cloth appearance or frame-time improvements.

### Background hair results across presentation resets

A deferred hair solve owns state and a separate mesh-write payload. Resetting
only its returned state left callers able to publish the pre-reset payload.
The native regression first failed on that stale payload; receipt after reset
must return the state without mesh writes, and a subsequent solve must still
succeed. Retain the normal background-versus-inline equality control. This
qualifies result invalidation, not a rendered scene transition or cloth quality.

### Hair dispatch ordering under a worker budget

The hair dispatch selector partitions to the available-worker prefix before
sorting that prefix rather than sorting every positive-wait wearer. Its oracle
retains the previous stable full sort: descending `total_cmp` priority, input
ordinal ties, unchanged eligibility and empty, partial and oversized budgets.
NaN/infinite distances and equivalent priorities remain ordering controls.
Isolated selector comparisons do not establish animation frame-time or FPS gains;
the waiting population and selection vector still use crowd-sized storage.

### Existing cloth defects and qualification limits

The recovery audit found two existing cloth failure families across nine methods:
cloak/mantle collar fit infeasibility and skirt/thigh face crossings. Exact-main
debug reruns matched the recovered-source failures. They were not recovery
regressions, but were not passing tests either; release repetition did not supply
an exact-main release comparison. The
[handoff failure inventory](../plans/procedural-model-handoff.md#known-failures-and-verification-scope-13-november-2026)
retains the named failures and bounded qualification. This historical comparison
is not a claim about later revisions.

The native verifier was blocked by display/socket setup, separately from those
geometry defects. A successful 3D smoke check did not execute the failing suite.
Keep native image review, geometric tests and CI smoke labels distinct. Sampled
rest/walk captures do not prove continuous motion; numerical clearance alone
does not establish art quality.

Other historical limitations must not disappear with the execution diary: the
padded-glove review left four knuckle pads and an oversized cuff unresolved;
panel review left Mage waist gaps and Knight neckline issues outside its scope.
The sleeve review did not establish a new hole or a continuous-joint cloth fix.
These are retained limitations of those reviews, not newly verified failures or
claims that later work has resolved them.

### Short-garment posed fit, November 18

The unchanged native short-skirt/tasset owner reproduced the existing thigh
crossing defect: all three resting layer cases cleared, but six walk/strike
cases contained 1,507 crossing triangle pairs. These are samples of the existing
root cause, not 1,507 distinct defects. A shared, instance-owned thigh-following
field reduced those sampled crossings to zero without changing joint poses.
Matched front, quarter and rear captures retained the skirt silhouette. The
first candidate lifted cloth through the belt; an upper-attachment regression
and matched rerender rejected that version before pinning the attachment band.
The authored front slit remains, rather than deleting or closing panels to hide
contact. Ordinary construction, layering, ownership, immutable-rest restoration,
scene remapping, topology and shared-material controls accompany the change.
Robes, long garments, aprons and separately fitted Orc/Troll hip decorations
retain their existing owners; the bestiary Red Arcanist stays on its fitted robe.

This is focused native geometry and sampled image evidence, not continuous
collision certification, a full native suite or hosted CI qualification. Existing
cloak and belt issues outside this slice remain unresolved. Initial OpenGL
capture failed its depth shader; software Vulkan produced one image before the
process was killed. Extending the existing software DX12 frame wait to software
Vulkan passed a first-red backend-selection regression and completed the matched
capture runs. Hardware/backend exclusion controls remain. No hardware timing or
performance improvement is claimed from that capture fix.

### Baldric stand-off from the torso

A Ranger fit witness samples emitted strap faces against the retained torso
support rather than trusting the already offset ribbon path. The old maximum
exceeded 0.017 model units against a 0.012 bound. Deriving the fit from the ribbon thickness
plus a small allowance corrects the excess while leaving the cloak's reserve
and the heraldry overlay intact. Existing loop, fold, buckle, apron, and
layer-order checks cover the adjacent contracts. Matched visual samples support
the closer fit; they do not establish universally flush contact or continuous
clearance against every piece of trim.

## Fixtures, tooling and evidence integrity

### Performance evidence locations

`EntryFormat.test_raw_location_rejects_normalized_path_escape` caught a validator
that accepted paths normalizing outside the project. Check each percent-decoded
path for dot segments, backslashes and control characters, while retaining safe
encoded names and ordinary dotted filenames. Many adversarial encodings exercise
one validation root, not many defects. This was local tooling validation, not
evidence that an asset was fetched or a published ledger was exploited.

### Schema and process fixtures

Wire/save changes need production-writer recordings, exhaustive-enum updates and
all client constructors updated together. A missing constructor field or stale
recording is setup/schema maintenance, not a gameplay regression. Preserve exact
identity, privacy, historical-memory and source-free recovery assertions when
updating expected cosmetics or approved contracts.

Server performance work encountered both a new fixture assumption and an existing
test API migration. Causal/display migration also exposed recorder-fixture errors,
a zero-test selector and disk-guard interruptions. These are not additional
production defects: fix the premise or harness, reject the zero-test result, and
rerun the registered owner. A compiler interruption with no terminal result is
neither a successful build nor a behavioral test failure. Keep such execution
details with the corresponding verification record rather than accumulating them
as new findings here.

### Engine-assigned family blocks, 28 November

Removing the authored `base_region` from procedural families (the engine now
assigns each family's identity block) left Python package generators, an
offline-authoring oracle, a stair-pair fixture and the frozen earned-route
comparator still writing or reading the field. **Stale/unupdated tests:** one
root cause, 26 distinct failing methods across 11 process-test owners, and 52
failed method executions in the PR's Linux debug and release Python
partitions. The Windows partitions failed the same methods; their executions are
not counted here.
The engine rejected each package with ``unknown field `base_region` ``, so no
gameplay assertion was reached. The generators now omit the field. The oracle
and stair fixture use the block the engine assigns. Comparator baseline v48
records the one-line source adaptation with a transition receipt, leaving v47
untouched. Rust tests had been migrated with the change, but its focused local
checkpoint left out the Python process tests that build packages. That
omission, not the contract, caused the failures.

### Tooling guards and native capture budgets

CI-owner probes exposed nested no-build guard assumptions in fixture shims. Child
fixtures now use their intended environment and mocks cover both dispatch
forwarding branches; version/audit, offline metadata and forbidden-command
assertions remain. This is harness isolation, not permission to relax the
production guard or replace actual-process assertions with mocked ones.

Native capture workloads can exceed GPU/readback budgets without a geometry
failure. Batching must preserve the exact ordered capture union and each capture's
causal prefix, while rejecting future events beyond the batch. A successful
render is still not visual acceptance. Keep the batching/schema witnesses
separate from the model and image-review evidence.

### Unregistered tooling test owner, 27 November

A full local Python run on branch `claude/code-docs-refactor-ovqedk` caught a
tooling defect introduced on that branch: a new architecture-check test class,
`test_check_architecture.MirroredSourceTests`, had no reviewed planner owner, so
the CI planner reported "unreviewed owner" and required full coverage. One root
cause produced six distinct failing methods in `test_ci_test_plan.PlanTests`,
with eight failed executions counting subtests. The class is now registered in
`ci_test_contracts.py` after a usage review, and all 23 planner tests pass.
Classification: production (tooling) defect, caught during development, after
it was pushed to the branch but before any merge. The same run's four errors
in arena-search owners were harness/environment failures (the isolated `optuna`
environment was not installed), not evidence about this change; all
four passed when rerun in a venv built from the hash-pinned requirements.
A later full run on the same branch failed one long earned-route process test,
`test_fixed_seed42_earned_hd_reexecutes_with_exact_source_free_restore_and_gross_flows`,
with `OSError: No space left on device`. Local build caches had filled the
session disk, and fail-fast left 3,334 tests unstarted. Classification:
harness/infrastructure, not a product or test defect. After the caches were
cleared, the full 3,553-test run had no failures on the same commit.
A second exhaustion happened later the same day: Cargo's incremental cache grew
by about 23 GB across repeated clippy, test and binary builds and filled the disk
during a selected process-test run. That run's log was truncated, so it counts
as neither a pass nor a failure. After the cache was removed, the same selection
was rerun; later builds in the session disabled incremental compilation.
Classification: harness/infrastructure. The full run on `0472df4` had
already finished before the disk filled. It ran 3,553 tests, and its only
errors were the same four arena-search owners, which failed with
`PackageNotFoundError: optuna`. All 17 tests in those three modules passed in
the pinned environment.

A selected six-module process run after the `advance_preparation` extraction
failed one method,
`test_projectile_process.ProjectileProcesses.test_ascii_secondary_weapon_stays_designated_when_swap_is_blocked`.
The inventory key returned no `inventory_rows`. The cursed swap is refused before
equip resolves, so the extracted equip step never runs on that path. The method
then passed six isolated runs on the same binaries.

A first diagnosis was wrong. It blamed the test for opening the inventory before
the window could accept a key, and it added the ready-frame wait that its
successful-swap sibling gained on 25 November (`1ad84e0`). With that wait in
place, hosted run 230 failed the same method again on `37c7e52`, in both the
Linux release Python partition and Windows release Python shard 2.

Root cause: a production defect in the ASCII client. The server publishes a
command's updates before its receipt. The Wield shortcut learns its removal's
intention only from that receipt, so it ignores a `Resolved`, `Failed` or other
terminal update that arrives first. `note_admission` also ignored the phase the
receipt carries. The swap then never advanced or ended, and its input stage
swallowed every key except Escape. A curse refusal can resolve in the admitting
commit, so the blocked-swap test hit this race; a removal that takes time does
not.

Classification: production defect (one root cause, one distinct method, three
failing executions: one local, two hosted; the platform repeats are not further
causes). The fix applies the
receipt's phase when it first names the intention. Three model tests in
`weapon_swap_admission_tests` cover a resolved removal, every ending phase and
still-running controls. Before the fix, the resolved and ending cases failed at
their receipt-phase assertions and the running controls passed. The readiness check
stays as an ordinary precondition, not as the fix.

A later run on `cda7ce01` exposed a separate driver error in the same method.
The final ASCII frame both reported the cursed dagger and satisfied every
readiness check; the first wait consumed it, and the second timed out waiting
for another frame. Wait for the refusal and readiness together before pressing
the inventory key. Keep the inventory designation and unchanged equipment
assertions. This is one harness cause, not another Wield defect. That second wait
was added on this branch, in `afcc2f0`.

### Headless fight wait read a compact frame, 27 November

A local run of CI debug Python shard 2 errored
`test_rogue_perception_process.RoguePerceptionProcesses.test_exploration_medusa_does_not_gaze_through_opaque_partition`
with `KeyError: 'state'`. The method passed three isolated reruns. The headless
client's stdout showed the cause. In compact background mode it writes a
`waiting` message without the derived state, marked `"state_omitted": true`.
`headless_fight_completion` indexed `frame["state"]` without checking that
marker, so a waiting message during a loaded run ended the wait with an error
instead of letting it read the next full frame. Other harness waits already
skipped compact frames.

Classification: harness defect (one root cause, one distinct method, one failing
execution). The predicate now returns false for compact frames. Its new unit
test failed with the same `KeyError` before the fix. Four other process-test
modules use the predicate and share the fix.

### Pre-existing 3D library failures, 27 November

A local `cargo test --manifest-path crates/client-3d/Cargo.toml -p tor-client-3d --lib`
on Linux (rustc 1.97.0) failed 20 methods on base `68bff10`, identically in
debug and release; 1,411 others passed. Bisecting the merge history:

- **11 stale tests, now corrected:** introduced by PR #87 (procedural model
  materials). That PR deliberately moved bracelets, armlets, earrings and
  necklaces from the `metal` builder to the new `gold` builder ("Adopt gold,
  glass, wood, steel, and chitin on necklaces..."), but bracelet, bracelet
  support, forearm plan, necklace and composition tests still read `metal`.
  They now read `gold`, and the layer snapshots they compare cover every
  bucket through `Parts::all_builders`. Three further tests were passing only
  vacuously, because their no-mutation and rejection checks never saw the gold
  layer. They now check it.
- **1 unresolved, from PR #86:**
  `garment_tests::breastplate_clears_emitted_underlayers_and_cloak_clears_plate_through_swing`
  passes at `8a083b1` and fails from `4ad85fd`.
- **8 unresolved, older than PR #86:** six `cloak::tests` and two
  `mantle_head_contact_tests` methods fail already at `8a083b1`. Production
  cloak fitting reports an infeasible radial fit by small margins (for example
  a 0.00018 collar deficit).

These nine are genuine fitting failures in independently maintained art
geometry, not stale expectations, and need that geometry's owner. Their tests
were not changed. Hosted CI runs only the 3D smoke check, not this suite.

### Documentation is tested behavior too

The documentation suite caught a renamed historical anchor still linked by a
creature plan and an obsolete protocol-version phrase presented as current.
Preserve stable anchors or update all callers, and label historical contracts as
historical. A test that correctly catches inaccurate prose is a documentation
defect, not a stale test to disable. Current format identities belong in the
[roadmap](../status.md), not duplicated throughout historical findings.

### Fitted accessory mounts and sampling

Crown settings lacked metal connections to their fitted bands, and crown/waist
jewels used global axes instead of their local support frames. Emitted-metal gap
probes and independent gem centroid/depth witnesses cover those defects across
support sizes and both body variants. Shared mounted ornaments now follow their
support; existing hair occlusion is not claimed fixed.

Conservative brow height windows, shared pouch grid samples and exact fitted-band
witness reuse reduce construction queries without changing support acceptance.
Retain full-versus-filtered hit equality, mesh/closure checks, and missing,
non-finite or excess-relief rejection. These are query-count reductions, not
measured frame-rate improvements or animation qualification.

### Native consumer receipt completeness

PR #6's native smoke succeeded but its evidence receipt omitted the explicit
`TOR_3D_TEST_BINARY` input. Recording that input repairs the harness without
relaxing the gate. The regression
`test_ci_receipts.ReceiptCompleteness.test_native_consumer_receipt_binds_the_selected_client_binary`
rejects the original incomplete receipt, accepts the repaired one, and rejects
reuse for a different requested binary. This is one infrastructure root cause,
not an accessory or rendering defect.

### Shared performance fixture schema coverage

The effects observation update missed the shared test-support performance JSON
fixture. Twelve integration methods failed during decoding because the required
`public_effects` snapshot was absent, before their trace-oracle assertions ran.
This was one stale-fixture root cause, not twelve gameplay failures. The fixture
now supplies an explicit empty snapshot; its schema witness verifies round-trip
validity and rejects omission of the field. Production decoding remains strict
and all existing expected-panic oracle checks retain their intended assertions.

### Hidden-contact spatial disclosure

The actual Service wand contract caught an invisible first contact being revealed
by the new ray endpoint even though ordinary combat receipts correctly hid it.
This is a production privacy defect, not a stale assertion. Ray and projectile
receipts now retain their actual stopping actor and require event-time target
identification before disclosing that path. The shared receipt regression covers
target-local, ray and projectile deliveries, later visibility and restored state.
The original Service assertion remains unchanged.

### Headless effect fixture reply ordering

Windows execution exposed the synthetic peer stamping a map reply with an old
observation cursor after sending an Outcome that advanced the stream. The real
client correctly rejected the mismatch. The peer now tracks the disclosed stream
cursor separately from the map's observation base. The real-process regression
forces this ordering and waits for accepted map output, rather than relying on
thread timing or ending the process before a stale reply arrives.

### Presentation-aware process fixtures

The nutrition process fixture's exact condition expectation needed the newly
required appearance/cue fields. A native ability-menu fixture also assumed the
message pager was empty after every action; added effect narration exposed that
assumption. It now acknowledges ordinary More pages and verifies each
acknowledgement leaves disclosed game state unchanged before opening the menu.
No gameplay assertion or pager behavior is weakened. The nutrition process check
passes locally. Native behavior remains for hosted qualification because the local
virtual display cannot open its socket; that setup failure is not a test pass.

The new spatial-receipt restoration witness initially tried to checkpoint before
sealing event observations. It now completes the normal observation transaction
before restoring; its hidden-contact and restored-state assertions are unchanged.

### Dependency update consistency

The SQLAlchemy 2.1.8 Dependabot update changed the hash-pinned environment but
left the sampler's strict allowlist at 2.1.6. The existing allowlist/requirements
contract test reproduced the mismatch (one failing method, one root cause).
The allowlist now matches the lock; additional witnesses reject a 2.1.6 installed
environment and retained history even with valid integrity hashes. This preserves
strict reproducibility rather than accepting mixed dependency versions.

The later grouped Python update in PR #75 reproduced that same mismatch on four
pins in one failing method. The CLI witness now uses the immediately previous
environment's real package tuple and still checks that rejection preserves the
input file. The separate CI usage admission failure was the reviewed manifest
drift check working as intended; the run did not reach sampler tests.

### Many-actor checkpoint fixture synchronization

Windows debug CI exposed one stale-picture fixture failure in the many-actor
retention/restart test. The action helper legitimately returned a started picture
at tick 13160; the final saved and restored picture was tick 13200. Captured final
pre-restart and restored payloads matched exactly apart from connection-local
snapshot/revision identifiers. The fixture now waits for tick 13200, completed
work and current input permissions before the save barrier. Exact restored-state
and history assertions remain unchanged. Two deterministic stream witnesses reject
started, observation-only and stale-permission frames and retain already-completed
pictures without waiting for another publication. Local process validation with
older protected binaries hit a rules-import setup mismatch; hosted execution on
the final head remains required.

The initial standalone checkpoint regression class also exposed the CI planner's
fail-closed unregistered-owner check (four failing planner methods, one ownership
registration root cause). The same two assertions now live under the existing
reviewed `test_process_harness.ActionCompletion` owner. Planner selection and
coverage gates are unchanged; no tests were dropped.

### Effect appearance descriptor reconstruction

The instantaneous effect wire round-trip test exposed one stale-oracle root cause
when `appearance` moved into `presentation.form`: its expected JSON path and its
required-field removal probe still named the old field. Both assertions were
updated to the new contract without dropping the missing-field rejection check.
One failing method appeared in two local executions; the focused three-method
wire owner then passed. This is fixture migration evidence, not a production
regression or full protocol qualification.

### Effects reconstruction: delayed observer receipt regression

An ordinary encoded delayed projectile release failed to deliver its frozen public
arrow appearance to a stationary attached observer. The new server transport test
failed once at its intended assertion, then passed after observer selection was
extended to newly resolved spatial events. The existing ordinary stationary Wait
fast-path regression also passed, preserving zero unnecessary observer receipt
work when no effect occurs. This is one production root cause and one first-red
method, not full effects, archive-compatibility, native-rendering, or CI qualification.

### Cell-bearing plan schema witness

The reconstructed cell-targeting simulation run exposed one stale schema oracle:
the strict ray-plan test still listed `direction` after the plan acquired the
closed `aim` record. The test now requires `aim`, checks its exact directional
encoding, rejects the old top-level direction, and retains every missing-field
assertion. This is one stale-test root cause and one failing method, not a
production defect or a weakened compatibility gate.

### Authored effect source placement

The combined real-process witness first failed during strict scenario validation,
before gameplay: two visual descriptors had been attached to same-named talents
rather than abilities. The fix moves those fields to the ability overrides in
both Rogue sources, preserving strict unknown-field rejection. A compiler-backed
regression checks the seven intended ability descriptions in both packages.
This is one authored-source configuration defect; the setup failure is not
positive gameplay coverage. Its process rerun and the new Rust regression must
be reported separately when executed.

## Scale-mail tip frame, 13 November 2026

`creatures::geometry::armor_parts::tests::scale_tips_point_down_and_lift_away_from_their_support`
exposed one production geometry defect: the scale outline stores its tip at
negative local Y, but its local positive-Y frame pointed downward and outward.
The resulting tips pointed upward and inward. The retained first-red regression
failed on the upward tip before the frame correction; it checks four cardinal
support normals, two row offsets and the closed five-sided plate topology.
This is one root cause and one distinct failing method, not eight defects.
The correction keeps the existing shared surface lattice and plate dimensions;
it does not change armor fit, layers, motion or cloth simulation.

## Effect playback and late cached instances, 15 November 2026

Fixed-tick native captures exposed two distinct presentation defects:

- Repeated floating-point delay subtraction left a blank frame at the eighth
  50 Hz tick between contiguous 80 ms cell hops. The ECS regression
  `creatures::effects::authoritative_tests::contiguous_hops_never_blank_at_fixed_tick_boundaries`
  reproduced that exact boundary. Integer `Duration` playback preserves the
  remaining time and removes the gap without changing disclosed geometry.
- A flying arrow reused after a ground arrow could cast a shadow while its
  visible surface disappeared. An isolated moving arrow and an elevated static
  arrow were positive controls. The CPU mesh was already extracted because
  procedural geometry is render-world-only, so the late cached instance lacked
  finite local bounds. The regression
  `creatures::effects::authoritative_tests::cached_projectile_meshes_keep_bounds_after_cpu_extraction`
  fails after removing CPU geometry while retaining reusable cached handles.
  Retaining small local bounds with each model fixes late attachments without
  retaining CPU vertex data or disabling batching.

Both regressions failed before their respective corrections and passed after.
Identical software-Vulkan captures confirmed the arrow surface reappeared and
that the eighth-tick gap disappeared; separated disclosures stayed separated and
instantaneous effects expired. This is local rendering evidence, not hardware
GPU, Windows or hosted-CI qualification. An earlier batch-isolation hypothesis
passed its structural assertion but did not fix the image; that ineffective
code and assertion were replaced, not counted as another production defect.
The capture-track schema test is feature coverage, not a third defect.

## Integrated format qualification, 15 November 2026

The reconstructed effects/targeting boundary records wire 72 and archive 70
with the real recorder and writer. A full protocol integration run found one
stale explicit revision oracle (`current_wire_protocol_revision_is_explicitly_pinned`)
still expecting 71; the correction changes that pinned expected version to 72,
not the version rejection behavior or message assertions. This is one test
migration issue, not a production regression.

Wire recording now has an explicit server-contract owner for the real paused
attack to drink replacement Outcome. Its selector test first failed because the
new factory did not exist; the actual server/headless case then passed, including
intention identities, potion consumption, elapsed tick payment and cleanup. The
original ASCII menu/narration method is unchanged and still graphical. Its local
run could not open a display and is not credited as passed; normal CI retains
that distinct UI coverage. The fixture was not synthesized from the expected
payload, partially exported, or recorded by a special hosted job.

## Headless recorder ownership, 15 November 2026

The utility and physical-projectile recorder selector tests first failed against
the old Text selectors. Dedicated headless process witnesses now provide raw
server samples after complete cleanup. The real-process checks reject any Text
or ASCII launch, and verify utility anonymous/persistent state and projectile
source queries, payment, exact dagger/arrow appearance, durable dagger recovery
and source-free reopening. Initial new projectile driver attempts failed on a
missing advancement overview and a misspelled wire aim tag; those setup errors
were corrected and are not production defects or regression evidence.

Text projectile and native methods are unchanged. The utility server scenario
is shared through explicit before/restored boundaries; its Text test still
asserts both status strings at each boundary. Recording does not claim Text or
ASCII verification. Other existing Text-backed recorder owners remain staged
migration work, not silently relabeled headless coverage.

The second migration slice moves Polymorph and Detection/Mapping recorder
selection to dedicated headless process cases. Both selector assertions first
failed against the Text-backed selections. Actual guarded recorder-owner runs
passed while rejecting any Text/ASCII launch: Polymorph preserves payment,
source deletion, exact captured-request replay and private-ledger exclusion;
Detection preserves nonempty/empty acquisition history, real paged Mapping and
source-free spectator reconstruction. All 36 pre-existing magic-item test
methods remain byte-identical. These are additional boundary ownership checks,
not newly discovered gameplay defects or reruns of the preserved Text tests.

The final ownership slice supplies headless Amulet, self-device, weak Drain,
hallucination, torch and discovery owners. Selector expectations first failed
against the presentation-backed selections. Original client test methods and
UI assertions are preserved. Guarded real-process recording succeeds without
Text/ASCII launches, including historical/visual state, payment and recovery.
New driver qualification corrected the torch fixture's held-light socket and
used the existing terminal-aware settlement mode for a verified Amulet victory;
these are setup/driver corrections, not gameplay fixes. The unchanged victory
assertions still require terminal success and the same carried identity.
Fixture bytes are not rewritten by this ownership migration. Complete recorder
qualification against the final integrated runtime is separate from these
individual-owner results and must not be inferred from them.

Following the test-architecture clarification, all remaining wire producers were
split from their Text acceptance owners. A complete local recorder run with a
global launch guard rejected any Text, ASCII or 3D client attempt and succeeded
using 41 real server launches and 74 headless client launches. All 82 server
sample kinds were retained, with 64 maintained client schema examples and nine
captured client queries. Ordinary protocol comparison then passed all 308
integration and 88 library tests. This qualifies genuine wire samples, not local
native UI behavior; the original presentation-client test methods remain intact.

### Server-only AI control stays headless, 16 November 2026

The autonomous self-defense owner still used Text solely for wizard arena
control. Missing that unnecessary executable exposed a harness/ownership gap,
not a failed gameplay assertion. The owner now sends the same step/resume
commands through headless and rejects presentation-client launches. Reservation,
start/resolution payment, exact restored resources, completion tick and surviving
condition assertions remain unchanged. A guarded Text-backed mutation fails at
the ownership boundary; the headless process passes. This corrects one test
owner, not every historical server test or any presentation-client behavior.

### PR #11 full-profile Python feedback (2026-11-15)

The first Linux debug Python shard exposed eight distinct failing methods
(nine failures including both natural-roll subtests), not nine production bugs.
Two exact catalog oracles omitted the newly authored visual/style fields; their
mechanical definitions remain exact. Four process methods assumed an exploring
practice target stayed in touch range. Their local fixtures now explicitly wait,
without changing generic fixture defaults or production AI. After that correction,
the Text self/ally method exposed a second fixture defect: Mend inherited hostile
interaction semantics from its bolt template even though its AI hint was friendly.
The supportive fixture now explicitly authors non-hostile interaction; all target
selection, neutral refusal, payment/completion and restart assertions remain.

The native strength-loss witness submitted another action before inspecting the
rendered turn. CI retained the correct weakness message in the preceding turn's
log. The driver now submits just the strength-loss action and checks its rendered
turn before any later action can archive it. Native presentation qualification
remains required in ordinary CI, not replaced with a headless claim.

The Rogue round-trip oracle incorrectly equated the player's visible Wait with
the entire world's durable endpoint. Retained history showed two undisclosed
actors moving afterward. The oracle retains exact branch, tick, durable tail,
public subsequence and source-free restart comparisons, and now requires the
public endpoint to be a paid Wait. Any later entries must belong to other actors,
be scheduler-authored autonomous admission/start records, and have non-public
scope. Negative unit witnesses reject own-actor tails, public tails, wrong authors,
wrong work kinds and an unpaid or non-Wait endpoint. No save bytes or gameplay
were changed by these corrections.

Focused local requalification passed 19 catalog/history tests, 20 documentation
tests, both natural-roll subcases, headless and both Text self/ally paths, and
the complete affected Rogue walkthrough. The two native methods remain CI-only
in this environment. These are focused local results, not current-head CI.

### PR #13 performance ledger identity feedback (2026-11-15)

Linux debug Python CI exposed one tooling defect in one existing test method:
the new ranged-loot comparison used its hyphenated CLI key as the ledger workload
name, violating the established lowercase/underscore identifier contract. The
canonical ledger name is now `ranged_loot`; the `ranged-loot` command selector and
strict ledger validator remain unchanged. A new actual-CLI round-trip witness
failed before the correction and now checks the independent canonical name,
written counts/percentiles, raw hash, validation and duplicate rejection. This
is a performance-report integration fix, not a gameplay, fixture or timing change.

## Model consolidation on the November 15 main

A broad client-3D library run exposed 15 failing methods after focused model
geometry tests passed. Each failure was rerun individually on the changed source
and unmodified main `5b9299c4`; all 15 reproduced with matching panic details.
This isolates observed failures from the refactor but does not make the full
suite green or rule out untested regressions. The baseline full suite was not
rerun. The unresolved groups are bounded cloak/mantle contact, centaur cuff
probes, composition/belt selection, moving skirt contact and pile input. Retain
their assertions and investigate production, fixture and driver ownership before
changing them. Passing bitwise geometry oracles cannot replace interaction and
actual-motion coverage. No known failure is waived by this comparison.

### Four stale model/input expectations corrected

Three stale-contract causes affected four methods: two centaur cuff witnesses
sampled beyond the padded glove's actual wrist-plus-0.075 lip; the Sylvan
composition witness required a cloak for the deliberately uncloaked female
recipe; and the pile guard witness rejected the legitimate active device-cell
aim command. The corrected tests retain angular/body sweeps and clearance limits,
assert both female and male cloak contracts, and require exactly the intended
`ZapCell` command while preserving no-command guards for blocked input.

All four focused methods passed in the native candidate containing these test
corrections. Two additional proposed test corrections were not included: moving
the crossing-belt fixture exposed inadequate clearance, and correcting draped
sleeve ownership exposed another unexpected-metal composition. Those failures,
actual moving-skirt crossings and cloak/mantle contacts remain unresolved. The
four passes do not establish a green full suite. Production geometry, gameplay,
fixtures and thresholds were unchanged by this correction checkpoint.

### Composition arm-metal ownership corrected

One stale composition predicate omitted metal that existing cloth/regalia emitters
own: detached drape boundaries and attachment rings, Bishop cuff edge tubes, and
regalia epaulettes. The equality assertion now names those owners alongside plate
and armlets, rather than allowing unexpected metal unconditionally. The full
focused-and-seeded composition method passed locally through the previously
failing Sylvan and RoyalBride cases. Evidence is reused only for that contract
from a binary also containing unrelated, unqualified belt-motion tests; those
belt edits are excluded. No production source or geometry threshold changed.
Belt-motion and actual skirt/cloak contacts remain unresolved, and this focused
pass does not qualify the full suite or a rebuilt exact publication tree.

### PR #16 inventory and crossing-cord feedback

Current-head Linux debug Python CI exposed one integration omission in four
planner test methods: both newly added diagnostic validator classes were absent
from the explicit reviewed-owner registry, so complete-inventory planning
correctly escalated to full even for unrelated tooling changes. Independent
source/assertion/resource review classified the six synthetic methods as tooling.
Only their exact classes were registered. Added complete-inventory regressions
for reporter/shared-import selection on both platforms and unknown-class
rejection; verification-control changes still require full CI.

The corrected waist fixture separated ordinary belt coverage from actual
crossing cords. Unique-pigment equivalence and actual emitted-face tests exposed
cord penetration hidden by the former wrong accessory selection. The bounded
repair increases stations from 16 to 31 and center clearance from 6 to 8 mm,
keeping the 5 mm radius, three bands and bow. Frozen original geometry remains
an independent negative control; dense sampling across 72 combinations passed
with minimum 1.259 mm clearance, plus actual animated ownership and bow guards.
This sampling is not a proof of the entire continuous surface. Local evidence
covers ten focused owners/diagnostics, not a green full suite. Skirt and cloak
contact failures remain unresolved. The repair changes geometry and costs more:
isolated optimized-dev cord construction rose 94.6% faceted and 38.7% smooth;
software mesh projections are not native-render, full-costume or release/FPS
qualification. Those limits are not waived by normal CI.
### Split ranged-kit purpose loop (2026-11-16)

Production-roster diagnostics at the local rules63 checkpoint exposed one shared
eligibility/movement defect: a disclosed idle bystander blocking the remembered
first step to a compatible counterpart made an underfoot bow or ammunition pile
ineligible. A sidestep made the pair useful again and drew the seeker home, where
eligibility disappeared. Both Hobgoblin and Centaur repeated this loop with either
half underfoot; unobstructed controls acquired their normal secondary kit.

The correction retains existing loot choices as the first pass. Only its failure
allows bounded ranged-pair recovery, with pairing and movement consuming the same
completed disclosed approach cache. Unit boundaries cover unknown counterparts,
occupied/disclosed cells, frames, permissions, exposure, shared-budget exhaustion,
destination deduplication and order. Real-process regressions independently require
the initial pickup, both finite acquisitions, secondary equipment, no oscillation,
an unchanged idle bystander and restart persistence. Deliberate regression
first-red executions are not additional naturally detected test failures.

Short actual-roster tactical probes also found meaningful existing distinctions:
Dragon withheld an intercepted flight while Medusa's gaze remained usable, and
Rattlesnake/Vampire selected their native effects. Those observations did not
justify changing their tactical policies. Neither the kit correction nor these
bounded probes establish complete obstacle-aware pathfinding, globally optimal
combat, new interests for every creature, or whole-scenario completion.
### Production medicine collection versus use (2026-11-16)

Local rules 64 roster diagnostics exposed a collection/use boundary mismatch:
known Rogue healing and extra-healing potions were supported by item-care use,
but ground pickup and the carried reserve recognized only simpler pure-healing
specifications. Synthetic fixed-healing pickup tests did not cover the production
per-hit-die healing plus condition-removal definitions. Full-health Hobgoblin,
Yeti, Troll and Vampire fixtures consequently ignored known medicine; unknown,
already-carried and care-disabled controls behaved as authored.

A separate reserve classifier connects collection to the supported production
medicine without broadening legacy immediate-Drink priority or changing use
thresholds. The ordinary-discovery unit witness executes an unknown Drink through
admission and scheduling, verifies consumption and actor-private learned identity,
then tests replacement pickup. Real-process witnesses deliberately seed prior
actor knowledge, require one finite pickup with the remainder preserved, no
healthy drinking, unchanged bystander knowledge and restart inventory. Unknown
medicine and care-disabled Emu remain negative controls. Seeded process knowledge
is not evidence that autonomous creatures learn by experimentation; scenario AI
cannot be taken over through ordinary client control. Deliberate first-red
regressions remain separate from naturally failing test counts.

### Completing anonymous investigations (2026-11-17)

A production Hobgoblin species/policy in the deterministic worn-Stealth process
fixture reached an ordinary aggravation-scroll location before expiry without
acquiring its hidden source, but retained the receipt. This left subsequent
Explore decisions liable to return to the already visited point. The new unit
and process regressions first failed specifically at retained-alert arrival,
not at package setup, disclosure or movement. Completion is owned by committed
actor-outcome reconciliation and checks both transition endpoints; it does not
add hidden target knowledge or a serialized goal field. Unit witnesses cover
preview/unpublished-outcome purity, arrival and relocation, newly received
current-point cues, remote fresh cues and unrelated transitions. The process
witness covers ordinary scroll/movement, exploration departure, fresh-cue reuse
and source-free restart. It uses a production species with a deterministic
Stealth fixture, not a generated full-roster birth or proof of general patrol,
territorial, investigation-search or coordinated tactics.

### Exploration continuation and route-interior novelty (2026-11-17)

The retained-destination checkpoint exposed two limited policy weaknesses.
Successful erratic movement could be immediately undone by the next ordinary
route toward its old idle goal; fresh destination ranking ignored how much of
its route had already been visited. Deliberate first-red tests failed at retained
versus released draft goal and at the independently verified corridor destination,
respectively. These are two policy regression witnesses, not additional naturally
failing-test counts. New boundaries cover exact erratic draws/direction, declined
chance, cancellation atomicity, pending checkpoint execution, unequal route
lengths, endpoint priority, seeded ties and saturated visit arithmetic.

The refinement releases only the draft idle goal after a selected erratic move.
For new goals only, equally visited endpoints compare mean interior visits with
exact integer arithmetic. Existing continuation, priority order and the 16-query /
512-additional-node limits remain. Comparing additional equal-tier candidates can
increase average work within those limits; no speedup or performance-neutrality
claim follows from the unchanged caps.

Eight paired actual-process probes use open/partitioned rooms, seeds 42, 46, 53
and 71, eight real Rogue species and 256 ordinary player Waits behind an opaque
wall. The original four checkpoint runs reproduce all journal fields after
corresponding generated entry/branch identifiers. Equal-weight means across four
nonerratic mobile species and eight cases are distinct cells 120.16 → 124.47,
same-direction run 2.830 → 2.838, reversals 1.094 → 1.063 and arrival-inclusive
Waits 28.53 → 29.31. Wall-band residence rises slightly, 34.1% → 35.1%.
Bat distinct cells rise 127.5 → 152.5 and reversals fall 28.13 → 20.63;
each improves in seven of eight cases, not all. Other mobile species' case-mean
coverage improves in five of eight cases, with small declines in three.

The erratic-only ablation has Bat cells 140.88 and reversals 23.63, while other
mobile coverage is 116.38. This separates the interventions without asserting
that their interaction explains every trajectory. All cases preserve inventories,
keep all actors alive, show no combat events, and leave Ice Monster, Venus Flytrap
and Xeroc at exactly 256 Waits and zero moves. Pauses break reversal/direction-run
segments; wall residence is accepted-tick-weighted anchor occupancy, not body area.
On the original four cases, nonerratic coverage remains 123.31 versus the older
pre-goal policy's 171.25. Purposeful continuation still trades coverage for pacing.
These quiet diagnostic layouts do not establish all-roster balance, full-dungeon
behavior or whole-workload performance. Process elapsed times are not a benchmark.

### Stationary acquisition and disclosed flight recovery (2026-11-17)

Two bounded production-roster scenarios exposed distinct targeting weaknesses.
A rooted Venus Flytrap repeatedly chose Wait while a nearby foe was in legal
Clutch contact, because both opponents had unavailable pursuit routes and a
farther, lower-ID opponent won acquisition. Near-low-ID and only-near controls
independently performed Clutch. Fresh acquisition now prefers disclosed melee
contact only for stationary actors; moving actors, retained fallback threats,
existing engagements, awareness and disposition gates keep their policy. The
unit and ordinary-process regressions first failed at action selection after
independent disclosure and contact positives.

A production Dragon declined its first target-body bearing because an allied
one-cell Snake intercepted it, but never considered the disclosed upper target
cell. An ordinary manual upper-cell breath harmed the Warrior without harming
the Snake; the lower-cell control harmed the Snake instead. Fresh flight choice
now reuses one disclosed canonical chart and, only after a definite primary veto,
checks at most 16 unique alternative target locations. Each needs positive clear
prefix evidence within effective range. Unknown, wall, alias and folded cases
cannot justify a new aim. The ray and ordinary-process first-reds reached the
expected choice assertion; no hidden collision or opponent-defense preview is
introduced. Launcher, exact-range, duplicate/cap and disclosure-boundary tests
exercise the shared mechanism.

Retained tests distinguish elapsed ray preparation from an earned multi-strike
prefix: rays do not have the latter. Pending and paused work preserve ordinary
queue/checkpoint authority and frozen aim; cancelling queued work leaves
authoritative AI and behavior unchanged. Independent review caught a
draft change to stationary retained-work fallback threat; its explicit guard and
assertion prevent that unintended scope expansion. That review finding is not a
third naturally observed production defect.

At seed 42, repeated corrected traces match 36 Flytrap and 6 Dragon accepted
journal entries after generated entry/branch ID correspondence only. Unchanged
Flytrap controls match 30 and 36 baseline entries; manual upper/lower Dragon
controls match 6 and 16. All other accepted fields remain exact. Both corrected
Dragon repeats reduce Warrior Health from 18 to 0 while the Snake stays at 8.
These are bounded trace witnesses, not general determinism or balance guarantees.

Probe setup errors are recorded separately: an external bystander initially
stalled scheduling, a public row used the wrong coordinate field, sequence zero
had no checkpoint, and a manual-ray driver initially overran terminal admission.
Corrected positive fixtures precede the intended failing assertions. Fixed-seed
lit rooms and one known ally do not establish whole-roster combat balance,
reflected/downstream safety, full-dungeon behavior or performance improvement.

### Known haste-device hostility hint (2026-11-17)

The generic known-device fallback correctly obeyed an incorrect authored hint:
Rogue's haste wand was eligible as hostile work although its effect benefits the
recipient. An ordinary Read/Identify unit fixture learned both type and positive
instance charges without a build grant. Independent ordinary Zap assertions proved
charge payment, Hasted, shorter target movement and checkpoint equality before
the intended first-red showed AI proposing that same Zap at its foe. Unknown
identity and unknown-balance twins preserve observational and decision equality.
The fixture uses synthetic actors and an explicit high-Intellect modifier for a
deterministic failed target dodge, not production NPC statistics or birth stock.

Only the hostile AI hint changes in Rogue and rogue-exploration. Content tests
preserve ordinary interaction, invocation, range, resistance and haste effects;
this adds neither self-buff selection nor new device reach support. Current NPC
births do not supply known charged wands, and the process protocol has no supported
external-to-AI control switch after Identify. This is a supported simulation-state
boundary, not a demonstrated normal-birth or end-to-end autonomous process case.

Engine semantics and format identities remain unchanged. Canonical rules pinning
means existing saves keep their old authored hint; the correction applies to new
content instances, not an implicit old-save repair. Existing source-free rules and
catalog process tests plus the saved-rules streaming test cover that mechanism,
not a directly observed old-haste/new-haste pair of saves. Frozen earned source
baseline 42 depends only on the unchanged bundled rules and remains valid.


### Canonical flight-box work reduction (2026-11-17)

This optimization preserves the original primary aim unless the existing full
chart proves its veto; it adds no attack policy, random draw, saved field or rules
identity. It avoids constructing that chart when all disclosed nonhostile body
rows are outside an inclusive canonical primary box. Unit tests compare the new
fast negative with the unchanged full-veto predicate, including target aliases,
non-anchor interception, region/coordinate extremes and rotated raster prefixes.
The rotated test proves geometric containment, not a full rotated-world chooser.
The synthetic disclosure-row tests are distinct from the real-content witnesses.

Four isolated production Dragon/Warrior/Snake cases verify no bystander, an
outside-box bystander, an inside-box/off-ray bystander, and a blocked primary with
a clear upper-cell alternative. Ordinary baseline/candidate process pairs match
4, 6, 6 and 6 accepted entries respectively after generated entry/branch ID
correspondence only. Aims, ordinary Wait tapes, resolution receipts, ticks and HP
outcomes match: Warrior Health is 18 to 0 and every Snake remains at 8. Wizard
observation is read-only diagnostic inspection; these are not generated births or
full-dungeon coverage. Public snapshot/record equality in the measurement probe is
not itself proof of private RNG equality; simulation purity and checkpoint tests
remain separate owners.

Six balanced-order pairs of warmed complete Engine chooser previews measure an
outside-box reduction of 42.5–48.5% (paired median 46.8%) on this local machine.
They exclude setup and execution and do not establish whole-game throughput.
The initial no-bystander control suggested a 9.3% slowdown; twelve subsequent
ABBA/BAAB groups gave a 1.6% median increase, spanning -11.7% to +10.5%, while
identical-binary repeats themselves varied substantially. This does not establish
a stable large regression or prove zero overhead. Inside-box controls span noise;
there is no broad performance or tail-latency guarantee. Timing is evidence, not
a test threshold. The three new correctness tests and existing flight, AI, weapon,
checkpoint and awareness owners remain assertion-based.


## Disclosed low-wall flight selection (rules 70)

An isolated actual exploration-package Dragon at [1,2,0] chose the
ordinary East breath in the fixed seed-42 adjacent-wall diagnostic, although its
own observation disclosed the wall at [2,2,0], both target rows at [5,2,0/1], and
the complete nominal ground prefix. The single recorded breath resolved without
hurting either actor and spent cooldown. A separately recorded ordinary lateral
move followed by a disclosed aim damaged the Warrior. This is one decision
policy defect, not a ray-physics defect or a measured population frequency.

The adjacent-wall unit regression fails before the fix at the shot-versus-move
assertion after positive disclosure and legal-move controls. An initially proposed
middle-wall unit instead failed fixture setup: the ordinary two-cell eye cannot
see the ground cell immediately beyond that wall. That failure is not a second
production defect. The matching production manual upper-cell hit demonstrates an
opportunity but does not satisfy this guard's conservative complete-prefix rule.
That case now explicitly keeps its original aim as Unknown. A separately authored
three-cell caster with higher eye supplies a genuinely complete chart; its
alternate-cell regression fails before the fix at the exact East-versus-upper-aim
assertion, not by injecting missing cells.

Boundary owners cover disclosed versus raw terrain, missing suffixes, sticky
contradictory duplicate disclosures, folded and target aliases, endpoint wall
contradictions, off-raster walls, transformed canonical charts, known-device
selection, and retained alternate preparations through checkpoint restore. These
are selection proofs, not hidden collision, ricochet, whole-game balance or
cross-platform qualification. The production Dragon process regression proves an
actual legal northeast move followed by exact source-free save reopening. A
separate bounded continuation starts its own breath at tick 142 and hits at tick
242 after one additional ordinary Wait from the preserved tick-200 save. Initial
diagnostic expectations confused the 142-tick diagonal move with a 100-tick
cardinal move and the command-entry tick with the final simulation tick; their
failed assertions were retained and corrected without changing geometry or aim.

Six balanced release chooser pairs for each existing control preserve exact aims,
public-state/profile invariants and work counts. Paired median changes relative to
0044e214 range from -4.1% to +2.0%, with individual pairs spanning approximately
-16.3% to +16.4%. These noisy controls neither establish a stable large regression
nor prove zero overhead, whole-game throughput or tail latency. The changed wall
cases intentionally choose different actions and are not timing-equivalence
controls. Rules 70 has a genuine current-writer golden, current certificates and
explicit prior-rules rejection; that change left the archive and protocol formats unchanged. All
prior fixture bytes and historical earned-route outcomes remain preserved.

## Repeated autonomous Flytrap hold coverage

The existing Clutch/Tightening Hold implementation correctly preserves an
existing hold and its escalation; the new witness closes an actual-process
coverage gap rather than fixing gameplay. The tuned Rogue recipe, modules,
character kit and rules are copied unchanged into a lit adjacent encounter.
Twelve ordinary Waits at seed 42 establish one hold, repeat Clutch without
replacement, and advance escalation from zero to three. Source-free reopening
at the fixed sixth Wait preserves disclosed state/history and saved actor
records (including pending work), RNG, items and active-clock stamps. This is
not a comparison of every field of the full Game checkpoint or a generated-run
balance claim. Private saved facts assert the fixed tape; they never select
inputs or extend it until a hit occurs.

The fixture helper now accepts an explicit source path. Its historical default
remains `rogue-exploration`; selecting tuned `rogue` does not silently replace
that default or mix either package's modules and player recipe. Missing selected
characters fail clearly. New helper-contract tests first fail at the absent
explicit-source API, then pass alongside the existing defaults. The gameplay
sequence already passed against unchanged rules 70 before becoming a permanent
regression; no behavior, format or compatibility revision is implied.

## Wall approach, ally-safe flight and pending restore

The exploration-content Dragon process witness
`test_dragon_wall_approach_and_ally_safe_ray_survive_pending_reopen` composes
previously isolated behaviors: a known low wall triggers an ordinary approach,
then the moved Dragon chooses the disclosed upper Warrior body cell around a
Snake ally. Six fixed ordinary external Waits at seed 42 produce the NE move at
0, the upper-cell breath at 142, and resolution at 242. Dragon-local disclosure
is checked both before movement and at the pre-selection tick-100 boundary.
At tick 200 the breath is pending and the Warrior remains unharmed; after source
removal, exact disclosed state/history and scoped saved actor records, RNG,
items and frozen stamps survive reopening. The remaining fixed Wait pair resolves
the captured breath: Warrior Health 18 to 0, Snake alive at 8. This is a bounded
cross-feature integration witness, not a new behavior fix, tuned-Rogue balance
claim, full-Game checkpoint equality, or generated-level qualification. No rules,
archive or wire identity changes are required by this test-only addition.

## Hostile touch continuation

The tuned-Xeroc witness
`test_tuned_xeroc_continues_hostile_touch_after_recovery_and_reopen` preserves the
actual Rogue starter and species kit. At seed 42 the player already recognizes
the disguise, but two fixed Waits leave the ambusher passive. An ordinary East
bump at tick 200 spends the initial touch strike and recovery without translating
either actor. The first ordinary continuation begins at 284, with positive
preparation, rather than granting another immediate strike. Source-free reopening
at 300 preserves disclosed state/history and scoped saved actor records, RNG,
items and frozen stamps. One final fixed Wait resolves that captured attack and
damages the player. The companion
`test_tuned_xeroc_hostile_touch_journal_replays_without_source` uses the same tape
without a latest periodic checkpoint, positively requires accepted journal work
after the archived base, then verifies source-free replay and the same resolved
continuation. The rules70 baseline instead returns to Wait at 284.

This is a deliberate continuation enhancement, not a previously promised
post-touch contract. Unit owners cover initial and deferred touch, preview purity,
actual AI draft commit, invisible contact, friendly and zero-memory exclusions,
existing engagement, retained work, and rejected-input state preservation. The
process witness covers initial-ready continuation, not the deferred path or full
Game checkpoint equality. No balance, seed-sweep, whole-level or broad timing
claim follows from this bounded tape. Historical archive bytes remain unchanged;
rules71 rejects rules70 saves rather than migrating their behavior.

Two surrounding persistence witnesses avoid incidental fixture assumptions.
`default_enrollment_and_visited_history_survive_source_free_durable_reopen_and_rewind`
accepts only Move or arrival Wait, requires committed nonzero-displacement moves within each
fixed four-round tranche, and retains visit-history change and exact replay,
reopen and rewind comparisons. The streamed-checkpoint deferral witness explicitly
pins its seven-input finite package before play: its source-preservation contract
must not depend on whether an ambient compiled cache happens to exist. It still
requires real detachment, later detachment, byte-identical saved sources and
successful durable replay and rewind; new-source admission has a separate owner.


## Autonomous Medusa variant boundaries

The three Medusa additions to `test_rogue_perception_process` preserve each
package's actual starter, species, rules and purchases. They deliberately do not
assume that tuned `rogue` and `rogue-exploration` own the same ability: tuned
Medusa approaches and chooses its authored basic attack, while exploration
Medusa selects Confusing Gaze at range and resumes ordinary movement during its
cooldown. At seed 42, three fixed player Waits cover ticks 0 through 300, with
source-free checkpoint reopen at 100. Exact public state/history and scoped
saved actor records, RNG, items and frozen stamps survive reopening. The gaze
case positively requires retained cooldown and exactly one gaze start; the tuned
case requires two approach steps followed by basic melee. A full-height opaque
partition keeps the exploration Medusa undisclosed and allows only movement or
Wait, never gaze. Every arm requires actual autonomous work and has a fixed
request/tick/transcript guard.

This is bounded autonomous-selection and checkpoint evidence, not a new combat
policy, a full cooldown-expiry test, a gaze success-rate measurement or a full
Game equality claim. Historical external-control gaze mechanics retain their
separate owner. No rules, archive or protocol identity changes are needed.


## Player and NPC demand in generated caves

The generated-cave integration witnesses separate two legitimate sources of
materialization demand. A copied player-only package changes only generated
actor counts to zero, retains authored geometry and coins, proves that only the
player exists, and keeps the exact unbuilt-region boundaries 2, then 1, then 0.
The walk must observe generated coins and replay exactly after its source is
removed. This does not claim identical realized layout or item positions to the
populated package, since population changes may change generator RNG use.

The original populated package has a separate fixed 17-move witness. It requires
an actual queued NPC movement inside the first cave to reduce the unbuilt count
from 1 to 0 while every player move remains inside the entry hall. The test then
removes the copied source and compares exact replayed states for the player and
all NPCs whose decisions ran, plus the final build count and replayed-record
count. This replaces an accidental player-only build-timing assumption, not a
runtime behavior change or a relaxed upper bound. Isolated warm-cache and
cold-cache failures of the old assertion, and the causal NPC trace, are retained
as diagnostic evidence. No rules, archive or protocol identity change is needed.


## Fixed-contact process fixtures own their idle policy

Five creature inspection, rules-provenance and Text/native ability process
methods accidentally depended on an autonomous practice target remaining in
melee reach while its authored policy said Explore. Retained process frames
showed the target moving from adjacent to nonadjacent before paid contact
resolved; the equipped-weapon narration explicitly reported lost reach. This
was a stale fixture assumption after the exploration change, not a failed
weapon or damage calculation. Idle Wait alone exposed a second assumption:
subsequent retaliation polluted the single-outcome diagnostic and interrupted
the three-ability UI tape.

These fixed-contact witnesses now explicitly author Wait with zero engagement
memory, keeping the ordinary AI controller and scheduler. The shared helper's
default Explore and positive memory remain unchanged, as do autonomous combat
witnesses. A helper contract compares all other fixture inputs byte-for-byte or
structurally. Existing damage, provenance, preparation, payment, one-outcome and
restart assertions remain intact. The separate mixed-protection defender owns
its matching policy explicitly. This bounded practice setup is not universal
reaction suppression: awareness and authored fear/control effects still run.
Native presentation retains its actual-window test and required hosted check;
a headless replay is not a substitute for that boundary.


## Scenario hooks (scripting milestone 1, 2026-11-18)

Two production defects and one class of stale pins, counted by root cause:

- **Once-binding mask (production, simulation).** A fake-host unit test of a
  `once` binding found that retiring the binding left its hook kind's mask
  bit set, so later sites kept building facts for a kind nobody heard. The
  invocation now refreshes the mask whenever it retires a binding. One method
  failed; it now owns the regression.
- **Unconfigured scripted runs (production, server).** The tutorial
  walkthrough integration test failed with only the run-start and timer
  lessons delivered. A package configured its adventure run, and so its
  character set, only for creature-built characters, an objective or a unique
  retrieval. In a scripted package without those, every actor fact failed
  the default character filter and `ctx:win` had no run to end. Scripted
  packages now always configure a run. Engine-level work counters (sites
  checked, facts noted) and the installed bindings localized it. The same
  walkthrough also showed that a character without a creature build has no
  combat view, so clients cannot see a scripted victory; that was tutorial
  content, fixed by giving its character a build. One method, one root cause.
- **Stale version pins (stale tests, expected with the format change).**
  The then-new wire revision (73), archive revision (72), scenario format 38 and validator
  `tor-scenario-52` failed the explicit pin and golden tests until the
  matching wire samples and the archive 72 scenario golden were
  recorded and the pins updated. These are deliberate first-red pins, not defects.

Not counted: a bulk `format = 37` replacement that also rewrote one literal
in the new v46 baseline test was an authoring slip caught by that test before
publication.

## Scripted outcomes (2026-11-18)

Moving every victory and defeat into scripts exposed two defects, counted by
root cause:

- **Goals on unentered regions (production, simulation).** The amulet process
  test, replayed by the wire recorder, found Rogue's opening goal missing: its
  root invocation was rejected because goal validation required the cell to
  be in a declared region, and the surface exit's floor is not declared while
  the hero starts on floor 26. A goal's cell is disclosed only by key until
  seen, so the check was wrong and is removed; the retrieval engine test now
  asserts the opening goal and its exit cell. One method, one root cause.
- **Run-start victory (package scripts).** `dungeon-characters`' integration
  test found the companion's start-of-run victory missing: the old objective
  was checked when the run was configured, but the first outcome script only
  checked moves and pickups. Seal and companion scripts now also check every
  character at `run_started`. One method, one root cause.

Not counted: about two hundred test fixtures that rebuilt or rewrote scripted
packages without their scripts, and tests asserting the removed built-in
outcomes, were stale fixtures for the deliberate behavior change.

The first hosted run of PR #32 found three more defects in Python package
tools, counted by root cause:

- **Silently dropped outcome scripts (tooling).** Eight package builders
  rebuild a Rogue, arena or fixture manifest without its `scripting`, so
  `copy_declared` copied nothing and their packages lost the death defeat the
  engine used to provide. The corrosion-ray process test caught it: the run
  continued past the player's death to tick 300 instead of ending at 242.
  Those generators now apply the canonical death script, and `copy_declared`
  refuses a manifest that declares no scripts. One failing method; the guard
  has its own unit test.
- **Activation candidates without scripts (tooling).** The Rogue activation
  candidate snapshot read only the manifest, ruleset and modules, so every
  candidate package failed validation for a missing `scripts/outcome.luau`.
  It now retains and writes the declared scripts. Fourteen methods across the
  activation and earned-route process tests, one root cause.
- **Stale scenario format (tooling).** The format 38 bump missed the arena
  matrix generator and one Rogue bot fixture, which still wrote format 37.
  Every curated arena, search, starter-study and evaluation package failed
  validation. About fifteen methods, one root cause.

Not counted as defects: service-contract and wizard fixtures that copied a
scripted manifest without its scripts; amulet tests whose limit tables kept a
renamed method's old name, or that asserted the removed escape item and text;
the objective validation tests; and the independent 3D lock, which lacked the
Luau crates (the same setup failure as the rusqlite update below). The
benchmark examples, which still cleared the removed objective, were caught
locally by the performance comparison's release build, before CI.

## Late movement pictures

At 50 Hz the native player body and camera both moved backward when a timed
325 ms east step ended before its picture arrived. The owning ECS regression
compares prompt and late geometry and checks that a newer relocation can still
move the player backward. A separate projection witness applies same-tick
outcome and picture-delta messages: the live cursor advances before geometry.
Keep picture sequence separate from live time and readiness. These tests
measure body and camera transforms, not the rendered pose or network latency.


## Cosmetic rebuilds during locomotion

A worn-equipment change recreated the walking body half a cell ahead of its
played position and discarded its queued step. A native scene test retains the
costume replacement while checking the root position and the remaining leg
under a shared world anchor. A new stream still starts with no inherited path.
This checks scene placement, not rig-specific foot contacts or rendered pixels.

## Named-level random teleport scrolls

Two paid-read Engine witnesses showed an authored same-level destination was
excluded whether loaded or initially unbuilt. A different level with the same
display name stayed excluded. The fix uses pinned level IDs, loads only the
bounded destination set before selection, and preserves source-free full replay,
checkpoint recovery and a repeat read after rewind. The archive-version
boundary test caught a stale schema expectation, not a separate gameplay defect.

## Independent client dependency locks

The November 18 rusqlite update changed the server manifest and root lock but
left the separate 3D workspace lock stale. The hosted 3D consumer's `--locked`
build caught this before launching a client: one build/setup root cause, not a
gameplay failure. Refreshing only rusqlite and libsqlite3-sys in the independent
lock restores resolution. Dependency updates shared through path dependencies
must account for both locks; the normal 3D build remains the boundary witness.


### Concurrent actor input-boundary refusals

The live venomous-bite learning driver observed the player before submitting a
snake action. Windows release CI exposed the resulting shared-boundary race:
the server returned `rejected/stale_context` where the driver allowed only local
`stale_observation`. This is one driver-oracle root cause, not a persistence or
combat defect. Both explicitly stale outcomes now permit a fresh observation;
other refusals and unknown outcomes still fail. The same live witness still
requires actual lasting strength loss and learned identification of the
rattlesnake, so a run with only stale refusals cannot pass.


### Redundant acknowledgement diagnostics in finite theft recovery

Windows release CI reached the orc arm's final recovery checkpoint after 35
requests and 2,468 actor ticks, but its transcript exceeded the 16 MiB arm guard
by 25,208 bytes. This was a diagnostic-volume failure before the final saved-loot
assertion, not evidence that loot recovery failed. Raw ACK response snapshots
accounted for about 2.2 MB of that arm's output without changing shared client
state. Opt-in compact-background output now omits only their repeated derived
snapshot, retaining exact raw receipts, all ready/error boundaries, and every
observation/lifecycle update. The output guard, finite action limits and all
ownership/recovery assertions remain unchanged. Owning headless projection tests
cover receipt preservation, omitted construction and retained state boundaries;
actual process qualification remains a separate witness.


The ACK projection change initially missed two downstream process owners: the
headless projection allowlist and a Bind Wounds witness that deliberately checks
stock, HP and cooldown at the queued ACK. Hosted CI exposed both fixture
mismatches. The headless owner now also requires real admitted ACK identity in
compact output. The admission-time gameplay witness explicitly requests full
derived output and retains all of its pre-spend assertions. Both owners belong
in focused validation for future projection changes, alongside raw-receipt unit
checks; unrelated streaming/theft passes do not substitute for them.

## Hallucinated disguise duration versus cadence bookkeeping

A shared base-turn phase was passed directly to the appearance selector, causing
all perceived mobs and objects to reroll on every normal turn. The native first-red
catalog witness changed its creature at phase one. The corrected contract is an
independent `(2d10) × 100` active-game-tick interval, not a longer condition lifetime
or a speed-scaled number of turns. The catalog's original selector recipe remains
unchanged, preserving historical event fields as final selector indices.

Pure witnesses retain independent literal dice-pair boundaries, exact interval
endpoints, private staggered streams, full first minimum duration, extreme-age
bounded access, and lazy ring-generation cost. Live observation checks compare
both mob and item through a base turn and checkpoint restoration. Unequal base
periods retain partial age and 100-tick publication boundaries; legacy projectile
round trips must not reinterpret old selector values. Owner/follower wire tests
retain literal initial appearances and refresh delivery without treating every
housekeeping signal as a new disguise. Existing lifetime, hidden-information,
map-acquisition and replay contracts remain separate required owners.

PR #31's first hosted selected run exposed one stale cadence-oracle root cause
across five Python methods: the Service follower witness, headless and Text
cadence/reopen, monster-detection refresh, and acquired-map/paid history. They
still expected the old phase-one bat after only 100 active ticks. The revised
witnesses preserve the initial appearance at that age and demand a changed
picture at independently derived seeded interval boundaries; detection's private
channel remains distinct, and acquired history retains its actual earlier
sample. This is test maintenance, not an additional production defect or five
separate gameplay bugs. Native focused coverage had not exercised these Python
consumer owners; their final execution remains required evidence.

A passing duration test does not guarantee a different catalog row at each reroll,
all-pose visual quality, or an unbounded nonrepeating random stream. The documented
1,024-duration ring eventually reuses durations for exceptionally long conditions.

## Authored wizard item selection

Authored item spawning has an independent encoded-Service first-red witness:
a correctly authenticated wizard's identity selector was rejected as an unknown
command before implementation. That is an intentional new-feature boundary,
not a discovered gameplay regression. Selector tests distinguish identity,
allocated appearance and their conjunction, collapse truly equivalent aliases,
and reject shared-asset or mechanically different recipe ambiguity. Engine
witnesses retain normal charges, location, unchanged knowledge, atomic rejection,
checkpoint/replay and rewind. The Service owner separately checks player and
spectator rejection, private wizard history and real potion consumption; mere
creation of a correctly named object would not satisfy its effect assertion.

Acceptance audit exposed one local feature gap: Rogue starter and generated
mace, arrow and ring mail share both identity and appearance but have different
birth policies. Rejecting ambiguity was correct, but neither dotted selector
could reach those recipes. An explicit authored `recipe` selector now closes
that gap without guessing a default or overriding allocated appearance. The
encoded first-red rejects `recipe rogue.mace` at the old parser. Resolver tests
cover all three pairs and retain mismatch/ambiguity rejection; recipe-specific
engine coverage preserves charges, knowledge, checkpoint/replay and rewind.

## Private wizard context and relative placement

Contextual wizard reports have separate bounded wire/client owners and encoded
Service witnesses. They expose backend IDs and region-local coordinates only to
an authenticated wizard, without ordinary observation fields, action admission,
journal entries or broadcasts. Nearby inspection is restricted to the attached
actor's region and an eight-cell radius with capped identity lists.

Review of the initial implementation caught two pre-publication boundary errors:
a literal `here` outside a position slot must not change absolute-command retry
semantics, and a retained presentation can remain valid for a frozen read while
being too old for a new relative mutation. Exact-slot parser and authoritative
boundary regressions cover both. Relative requests resolve to absolute journal
operations; stale contextual retries reject rather than readmit from a later
location. Native checkpoint/replay/rewind witnesses and Service no-second-spawn
assertions remain distinct from client display/correlation coverage.

The first encoded-Service run found fixture/oracle errors in two new methods,
not production failures: chamber shells include x=-1; offset syntax/range errors
are more precise than a generic usage line; and every explicit snapshot request
issues a new presentation revision. Corrected tests first prove the shell tile
and a no-query snapshot increment, then assert the outside tile and the exact
same single snapshot increment after queries. Complete observation, readiness,
cursor, history and peer-output checks remain unchanged. Reusing a mutation
receipt ID for a read query still rejects with RequestConflict.

Item catalog/instance inspection extends the same private report path. An exact
retained pre-extension Service host accepts existing location queries and rejects
the new `item list` and `item inspect` requests at the intended unsupported-command
boundary. Catalog tests retain exact seeded allocation, separate recipe aliases,
and gap-free bounded pagination without generating items. Instance tests distinguish
remaining device charges after ordinary use from the catalog's original charge
recipe, and preserve location/ownership, discovery and checkpoint state across reads.

Resource setup is intentionally a positive restoration transaction for stamina,
focus and mana, never a raw balance/health/nutrition field setter. The retained
old Service host accepts existing context queries and rejects `resource restore`
as unsupported (feature first-red). New witnesses separately cover full-capacity
clamping, positive restoration after ordinary paid work, exact held resource and
condition reservations/preparation preservation, and deterministic replay/rewind.
Dead-actor denial shares the normal loaded/alive selector and restoration guards;
missing-target atomicity is tested separately, not claimed as a direct death test.

The combined resource checkpoint exposed one fixture lifecycle error in the paid
care restoration witness: reopening a save deliberately does not retain wizard
authority. Its later wizard restore correctly rejected with Unauthorized. The
fixture now explicitly enables wizard mode on that reopened engine before the
privileged request; ordinary completion, exact spent amount, reservations and
replay assertions remain intact (one failing method, no production change).

The first combined encoded run found two additional fixture/oracle mistakes:
this seeded invisibility device is a staff, not a wand; and obtaining the ordinary
player role frame itself issues a snapshot revision. The name oracle now matches
the authored seeded staff, and role frames are obtained before the independent
revision control. All charge, identity, ownership, observation, history and denial
checks remain; neither failure required a production change (two methods).

The wizard test adoption audit found one stale protocol coverage owner: the
exhaustive `ServerMessage` taxonomy in `protocol/tests/it/samples.rs` omitted
`WizardReport`, and the current wire corpus/recorder did not cover its kind.
The test match, required-kind list and recorder now include it, with one sample
captured from the unchanged production Service encoder. Existing wire samples
remain unchanged; this was a test coverage defect, not a production wire change.

The v73/Luau integration merge was textually clean but its active corpus lacked
WizardReport. The required-kind test failed at that exact boundary. The verified
report sample now resides in v73, leaving v72 byte-identical to merged main;
existing v73 scripted-message samples remain unchanged. Independent omission
controls detect the missing WizardReport kind and missing tutorial text/control
fact, then the restored corpus passes. The AI Give fixture also inherited an
external outcome-script declaration without supplying script sources; like the
upstream catalog fixture, it now removes only that unused scripting declaration.
Its tactical rules and existing medicine/knowledge setup are unchanged. Exact
integrated Service runtime remains a hosted qualification, not a local claim.

### Package invariants and bounded rewind retention

The every-package invariant test rewound to the initial state after twelve
played turns, assuming any package's history fits the engine's retained rewind
window (the latest 128 selectable states). The 26-mob peaceful menagerie logs
about 626 rewindable entries in those turns, so the engine correctly refused the
rewind. This was a stale test premise, not a production defect: one root cause,
one method (`invariants::every_package_rejects_retries_replays_and_rewinds_exactly`),
two failed executions (hosted Linux debug and release). The test now requires an
exact rewind whenever history is within the window. Beyond it, rewinding to the
dropped start must be refused without acting or consuming a sequence, and a
one-turn game of the same package must rewind exactly.

### Identify turn's scripted outcome (2026-11-24)

The hosted Linux debug Rust lane exposed one stale Text fixture: after a paid
Read, its mocked Identify updated item knowledge but omitted the server-authored
narration. The old test had relied on the client inventing a line from the
action. The fixture now sends the existing outcome message used by the server
and requires the exact line once, while preserving its no-extra-turn, charge
and request-count checks. The live service and source-free replay separately
check that the server produces and retains the cue. No production change was
needed for this test failure.

The next hosted Linux debug Rust run found one independent current-format
assertion still pinning save archive 72. Its expected archive is now 73;
the scenario, ruleset, component and arena formats are unchanged. The
separate physical-framing and previous-save rejection witnesses remain.

### Retained main plan serialization (2026-11-25)

The Identify merge passed selected PR qualification, but its fresh main
integration was rejected before running smoke tests. The indented retained
plan was 2,153,018 bytes, above the existing 2 MiB reader limit. The aggregate’s
missing smoke receipt was downstream fallout, not an additional root cause.
The producer now writes compact JSON and declines retention if that encoding
still exceeds the reader limit, leaving fresh CI enabled. The external evidence
bounds and both smoke requirements are unchanged. The regression tests cover
the production plan writer and both post-merge plan readers at realistic lane
sizes, plus fallback when the compact encoding is still too large.

### Interpolated walking speed (2026-11-25)

The first full simulation unit run under rules identity 73 found one stale
expectation shared by 22 test methods. Their SPD 0 actors still expected
the original walking pace. Updating the independent expected ticks preserves
coverage of pursuit and retreat, committed steps, occupied destinations,
confused movement, guise contact, and Haste/Slow composition. For example,
a 100-tick base diagonal rounds to 142 before SPD scaling and costs 178 at
SPD 0. One server unit test also expected the old retreat duration when replaying a scheduled mob escape. Across those two suites, 23 distinct methods exposed the same stale timing assumption. The deliberate first-red feature test is excluded.

### Overlapping projectile reaction cues (2026-11-25)

With two disclosed bolts crossing an actor, the death animation could
wait for the later cell crossing even when the applied ability and hurt
reaction pointed to the earlier flight. In the native scene regression
the hurt cue used 0.7025 s while the death cue used 1.28 s. Both now use
the applied ability's arrival when available. A separate control with no
applied cue still uses the outstanding crossing rather than reacting
immediately. This is presentation timing only; it adds no causal claim
to the wire events.

### Flytrap projectile contact (2026-11-25)

A lethal projectile notified the 3D scene before reaching the Flytrap.
The rooted death transition used the pose from notification time and
blended toward wilt immediately. The real-scene regression compares
a lethal bolt with a nonlethal bolt using identical timing. On the old
animation, the neck already differed by about 0.37 radians one frame
before contact at four cells. The fixed transition continues the current
bite until contact, or resumes the breathing pose if the bite has finished,
and starts its existing release from that pose. The regression checks both
distances without changing the projectile speed, reaction event order or
root geometry.

### Single-cell projectile disclosure (2026-11-25)

When a bolt left view and reappeared in only one cell, the renderer
started the endpoint fade immediately. Flight timing counted the hidden
gap but did not give that cell an arrival; hit reactions
could therefore start too early. In the native scene regression the
unlinked hit started at 0 s instead of 0.64 s. Single visible cells now
have an arrival on the same flight timeline as longer fragments. That
arrival schedules both the fade and the reaction, including any cast
lead, without drawing a path through unseen cells. A separate model
regression checks that a nearby smoothed line cannot claim a cell from
another fragment. Isolated no-lead points remain immediate.

### Target flash and cast contact (2026-11-25)

A disclosed Spark Lance target flash appeared on the first 20 ms frame,
even though the caster's impact frame was 382.5 ms into the animation.
The ordered native scene regression now requires no flash through 380 ms
and a visible flash at 400 ms. Anonymous flashes stay immediate, private
status stays off the world, and no missile is invented. The fixture also
checks the agreed timing when observed harm is already part of the same
scene batch. The service sends ordered outcome facts separately from
visual observations; this change does not move hurt reactions derived
from a later picture.


### Timed walk support across a long frame (2026-11-25)

At 240 ms of the same 325 ms cardinal step, an Emu reached about 0.738
cells along the path in both frame schedules. After warming up to 60 ms,
one 180 ms frame reset ground support and added zero gait phase, while
nine 20 ms frames kept walking. The production root and animation systems
now preserve gait only when both samples still match the same timed leg.
A real placement still resets footing, and playback tests reject first
samples, mismatched positions, leg changes and skips as continuity proofs.
Numerical smoothing and contact solver step limits are unchanged. This is
a system-level timing test, not a pixel comparison.

### Wardrobe slot ownership and mixed disclosure

The wardrobe checks cover the 34 presets and 51 gear identities. A synthetic
outfit with a separate cape and robe demonstrates why mere co-occurrence
cannot determine hood ownership. The equivalent shoulder test keeps the
same decoration but varies whether the plate is its mount. The emitted
bridal witness requires all three original cloth layers and preservation of
both the hem depth and rear train reach after adding waist armor.

The mixed disclosed transition exercises live actor reconciliation with a
shirt, plate and riding boots, then removes only the plate. It checks
selection parity, real shin mesh clearance, restored shaft height, and reuse
of the rig for unchanged snapshots. It does not substitute for sampled
visual review of pose-dependent cloth.

The forearm-cloth regression first failed on the Phantom with a replacement
shirt: the old sleeve tatters were still present in the resolved outfit.
The shared slot test covers both Phantom and Wraith and distinguishes shirts
and articulated armor from torso-only armor and bracers. It keeps the separate
hip cloth as an independent witness.

Two head-mesh tests exposed the same missing anatomy gate in the plain helm
and raised hood: each added hair-pigmented faces to the female Troll. The
negative oracle checks the covered head, since the uncovered Troll rightly
has its own crest. Positive witnesses retain that uncovered crest and require
Human braid and Human/Wraith lock geometry in the appropriate regions.

The headwear fit audit reproduced raised-cloak fabric clipping the Medusa's
serpent crown and the Nymph's earrings. The composition regression first
failed because the permanent crown still received a raised hood. It now
checks folded and raised states, preservation of the cloak and underlying
jewelry, and priority of a separate open hat.

A separate head-mesh witness found the Hobgoblin still emitted the mohawk
(140 sampled faces) or topknot (364) with either a steel helm or a raised
hood. The shared species coverage rule now suppresses the crest and folds
the ear tips under closed headwear. Bare heads and lowered hoods retain
the original silhouette.

The open-hat comparison exposed the inverse coverage problem for ears:
the fedora hid the crest correctly but selected the closed-helm ear
shape. A silhouette regression now requires exposed ear geometry under
each of the six open-ear equipment recipes while keeping the crest hidden.
The shared low sweep clears the fitted hats in an emitted-triangle test;
the unsupported bare sweep deliberately fails that clearance under the
fedora. Bare, uncovered and closed-headwear witnesses keep their roles.

The Hobgoblin portrait test measures the central bridge and tip in the
continuous face surface for both cuts and three builds. The old head with
a separately drawn nose failed at 95 mm of projection against the 112 mm
bridge minimum. The new sculpt has enough projection to read, with a
bounded front so it does not protrude beyond the steel helm.

### Hobgoblin idle and lookout poses

Two pose regressions reproduced one shared assumption: both the idle and rare
antic rotated the equipment arm even though a Hobgoblin does not necessarily
hold a club. The tests failed on the right-arm joint before either pose was
changed. The replacement gestures leave the grip alone, keep a readable head
motion, and do not depend on whether the head is covered.

### Hobgoblin strikes during braking

A real-client comparison replayed the same approach with no attack, a strike
at the stop, one during the settling steps, and one after a full stop. The
old early strike moved one ankle 0.122 world units between 40 ms samples,
against 0.027 for braking alone. An actual gameplay-frame test independently
reproduced the ankle jump. Hobgoblin strikes now carry the already-solved
feet into the shared short handoff. During braking, landing and remaining
on the ground are determined by the settling steps, not the last walk phase.
The matched review retained the full-stop and no-attack controls, action
phases and post-handoff pose. This is local motion evidence, not a timing
or floor-contact guarantee for all actors and speeds.

A separate restart comparison found the inverse discontinuity: returning to
walking just before a Hobgoblin strike ended made the feet jump once as action
support faded and again as world support was created. Restarting after the
strike did not reproduce it. The new gameplay-frame test preserves ordinary
and delayed restart controls. A moving Hobgoblin now establishes normal
world footing during the last support fade instead of rebuilding it after the
strike timer expires. Stationary strikes, action phases and root motion stay
unchanged in the matched review.

### Hobgoblin eye aperture

The emitted white of the old Hobgoblin eye measured 0.511 times as tall
as it was wide on the first male fixture, outside the authored squint
band. The regression checks real eye geometry on both body cuts and three
builds, requiring enough opening to stay readable.

The eye pass also reran the neck-root tests and exposed a stale idle-pose
fixture after the listening tilt changed. The six neutral Head matrices were
recaptured from the native client. The Torso and walking matrices were kept,
as were the mesh-depth oracle and all its tolerances.

### Extended model materials in darkness

A native fixed-camera probe ran the live model-shading system on a steel
helmet across a dark-to-lit transition. Before the fix, a curved interior
patch still had a visible highlight in darkness. The custom forward shader
now skips both lighting and surface detail for unlit materials, then uses
the normal fog and post-processing. The dark patch's mean-channel brightness varied by 26.45
8-bit levels before the fix and 0.27 after (standard deviation). The lit
comparison stayed unchanged. The ordinary bright-only shader smoke did not expose the defect.

### Held props in scripted model review

Two capture-only failures interrupted an armed Hobgoblin review. The script
rejected the model even though its biped rig has a right-hand attachment, and
an armor swap removed a Human's held sword with the old rig. The validation
test now distinguishes grip-capable body plans from handless ones and keeps
live playlists out of scope. The scripted scene test checks the initial prop
and the replacement grip in the same scene, including removal of the old prop
and preservation of the item's attachment transform. This does not change
gameplay equipment or require pixel-identical captures.

### Buried neck root after a wardrobe swap

A near-front Hobgoblin review showed skin through the closed bib just after
putting on a breastplate. Reprojecting the emitted head and torso triangles
with the independently captured joint frames reproduced the penetration and
separately confirmed that the exposed throat remained visible. The neck fit
now leaves more overlap only at the buried lowest ring. Native idle and
walking witnesses, already-supported roots, and open necklines keep their
existing checks. The offscreen review compares the settled, swap and strike
poses visually; it does not require matching image pixels.

### Measuring retained parts under replacement armor

The armor-and-cloak test compared a Knight's equipped breastplate against
nonmetal surfaces from the unequipped outfit, including trim on the
decorative cuirass that equipment replaces. The old check failed identically
with and without the neck-root change. The corrected witness samples metal
and retained underlayers from the same equipped torso. It still excludes
the intentionally over-armor baldric and retains the original coverage,
clearance and cloak-motion checks. No production change was needed.

### Readable dungeon darkness

A fixed-camera native capture of the old unlit shader recorded about 10% of
the lit view's central linear luminance. The first readable treatment reached
80% in the same scene and retained visible stairs, ground loot and a creature
at gameplay distance. The remembered route kept its stair and geometry but
not the creature or item; channel spread on its floor stayed below one 8-bit
level. The windowless capture uses the production cave renderer and shading
systems with authored disclosure snapshots, not a live playthrough.

The spectral material regression reproduced an emissive rim even after its
base emission was cleared. The dark material now suppresses that shader
contribution without removing the normal detail or mutating the lit recipe.

### Bridal veil floor contact before the final fall

The old floor test sampled the later stages of a fall. Extending the same
weighted-mesh witness to earlier frames reproduced a 44 mm penetration during
the stagger and measured up to 195 mm under the floor during the dramatic
wind-up. The torso can already bend while the final fall clock is zero.
The existing train support now handles the whole active death performance,
with a small allowance near its start. The check still covers cloth, trim and
gems, the settled floor and the pinned attachment. The living and
corridor-facing train poses remain unchanged. Matched native captures of all
three death variants kept body timing and joint poses while the trailing veil
cleared the floor.
