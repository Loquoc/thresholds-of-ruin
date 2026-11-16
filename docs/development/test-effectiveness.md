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
