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
