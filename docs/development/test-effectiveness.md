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
  Protocol 73, save format 72, scenario format 38 and validator
  `tor-scenario-52` failed the explicit pin and golden tests until the
  protocol 73 wire samples and the save format 72 scenario golden were
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
