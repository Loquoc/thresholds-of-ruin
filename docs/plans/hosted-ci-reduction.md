# Hosted CI review and reduction proposal

This review covers all four workflows in the migration source, the new separate
advisory workflow, their local
entry points, selected-test ownership, build transport, post-merge proof and
existing cost evidence. The local candidate removes two diagnostic workflows,
corrects campaign/encounter ownership and local prerequisites, gates draft
qualification, groups minor/patch dependency updates, bounds 3D log retention,
and replaces recurring hosted scans with explicit advisory dispatch.
Selected proof reuse and a single Linux debug 3D smoke job are implemented locally. Domain-specific selection remains proposed. Hosted savings are unmeasured.
The [testing policy](../development/testing.md) still owns qualification.

## Current migration boundary

The migration copy retains the source Git history, modified tracked files and
untracked files. The original checkout is preserved. The new repository is empty
and Actions is disabled at the repository level. No push, release, workflow run,
credential upload or repository transfer is part of this preparation. Do not
enable Actions as part of uploading source. Resolve account restrictions with
GitHub Support before deciding to resume hosted execution.

Removing a workflow from the working tree does not remove its historical Git
versions. Do not dispatch an old revision after re-enabling Actions. Any later
decision to rewrite history requires a separate preservation plan.

## Workflow inventory and assessment

| Workflow | Automatic triggers and workload | Findings / disposition |
| --- | --- | --- |
| `.github/workflows/ci.yml` | PR events, main pushes, release and manual runs. | Core gates and one Linux debug 3D build, launch and input check. Full art and gameplay verification runs locally. |
| `.github/workflows/dependency-advisories.yml` | Explicit manual scan only; one ten-minute Ubuntu job. | Pinned cargo-deny action checks root workspace advisories only, with blocking failures and read-only checkout. No application build, test matrix, qualification seal or PR/main trigger. This is a configured timeout, not a measured duration. |
| `.github/workflows/retention-diagnostic.yml` | Narrow PR paths and manual dispatch; one 90-minute Linux job, repeated release measurement matrix. | Removed. Keep `scripts/retention_measure.py`, its validators and the opt-in benchmark for local execution. |
| `.github/workflows/worn-equipment-review.yml` | Narrow review-script/fixture PR paths and manual dispatch; one 120-minute Linux build and render job. | Removed. Keep `crates/client-3d/scripts/render_worn_equipment_review.py`, fixtures and correctness tests locally. |

The two removals eliminate up to 210 configured job-minutes when both would
otherwise run; these are timeout ceilings, not measured usage or savings.
The independent 3D matrix has a 540-job-minute ceiling per event (two 150-minute
Windows and two 120-minute Linux limits), not a runtime estimate. Repeating that
matrix on main can double the configured opportunity for work.

### Core job review

| Job group | Purpose / existing control | Remaining opportunity |
| --- | --- | --- |
| `scope` | Full Git scope, docs validation and canonical policy; five-minute limit; read-only GitHub metadata. | Report why selection expands and why post-merge proof fails. Do not guess narrow scope on failure. |
| `tooling` / `dependencies` | Workflow/Python lint and advisory/license/source checks; ten-minute jobs. Dependencies are omitted for verified prose-only changes. | Keep fast blocking checks; separate lightweight advisory cadence from exhaustive testing. |
| `profiles` | Exhaustive Rust/Python partitions, Linux debug Python sharding; generally 40/60-minute limits, Windows release Rust 75. | Avoid launching the exhaustive plan twice for an identical qualified merge; diagnose compilation before adding shards. |
| `windows_binaries` / `windows_python` | Two profile-specific producers and four Python shards; source/config/byte-bound binaries. | Sharing already exists. Measure residual rebuilds before changing recipe keys or producers. |
| `selected_rust` / `selected_linux` | Application producer and fresh selected Linux tests; compatible binaries shared. | Current semantic Rust changes still select all ordinary process tests on Linux. Map smaller contract sets with independent witnesses. |
| `selected_windows` / `selected_release` | Platform witnesses/tooling and optimized sentinels/affected contracts. | Pure tooling is repeated across Linux and Windows; some source domains expand Windows to all ordinary process tests. Separate portable checks from actual platform behavior after review. |
| `selected_consumer` | One Linux debug 3D build and native smoke check. | The job uses the same recipe for selected and exhaustive core plans. |
| `postmerge` / qualification producers / `check` | Distinct exact-tree full/selected PR proof, fresh main integration, independent receipts and required platform aggregates. | Full and selected seals retain normal fresh fallback. New-repo proof is initially unavailable; hosted acceptance and fallback rates remain unmeasured. |

### Permissions, dependencies and external activity

All retained workflows have read-only repository permissions; selected jobs add
read access to Actions/PR metadata. Checkout disables persisted credentials.
Actions are pinned to full commit SHAs: checkout, artifact upload, Rust cache
and cargo-deny. Keep these protections. No `pull_request_target` execution,
OIDC deployment, custom service hosting, website task automation, incentives,
mining, remote tunnel or release publishing job was found in the workflow definitions.
Dependency installation and advisory access support this repository's development.
GitHub CLI calls retrieve this repository's jobs/proofs/artifacts; caches are
build inputs and are not accepted as successful test evidence.

The Python arena-search environment is hash-pinned, but installation is repeated
in many lanes, including lanes that may not need it. Audit imports before splitting
the environment. `.github/dependabot.yml` allows five open PRs per ecosystem
(Cargo, Actions and pip), with weekly updates. Those are open-PR caps, not measured
weekly counts. Minor/patch version updates are now grouped per ecosystem;
major and security updates remain independent and may trigger expensive fallback.

This is a source review, not a clean bill from GitHub. No suspension reason or
live old-account run history was available. Project-related rendering may still
be scrutinized for resource burden. Do not treat a new account, private visibility,
local runners or a billing budget as permission to bypass a restriction.

## Evidence and limits

The [existing measured baseline](../development/ci-cost-baseline.md) is from before
the current risk-based regime: a representative full core run used 4.3925 runner
hours; Windows was 61%, and Cargo build messages accounted for about 2.09 hours.
Some sampled PRs consumed 9–21.5 runner-hours over revisions and main runs.
Those observations identify likely bottlenecks; they do not measure this copy,
the new account or the benefit of already implemented sharing/selection.
Fresh hosted measurements are intentionally deferred while Actions is disabled.

### Confirmed owner-classification gap

Local validation exposed a concrete prerequisite/ownership problem before any
hosted pilot: `ci_test_plan.discover_inventory()` initially classified both
`test_rogue_campaign.CampaignProcess` and
`test_rogue_encounter_study.EncounterProcess` as `process: false`. These classes
inherit from `unittest.TestCase`, were absent from `APPLICATION_OWNERS`, and execute
real applications. The campaign test implicitly built all workspace binaries
inside a stage presented as tooling. Encounter tests ran before `tor-arena` was
available and failed. Two other methods failed because the system interpreter
lacked the required pinned search environment. These were setup errors, not
failed gameplay assertions. The initial quick checkpoint remained failed.

The unexpected campaign owner took about 503 seconds in that cold run, including
its build; this is a local diagnostic, not hosted cost or a controlled benchmark.
The local fix explicitly classifies both owners as application tests and reviews
their fixture/controller classes as pure. Local debug plans build ordinary
applications before tooling workers start. Discovery, whole-inventory selection,
prerequisite and real CLI-plan regressions cover the correction; unchanged
campaign/arena acceptance tests exercise the actual processes. No-build guards
remain intact. A fresh empty-target end-to-end rerun is not claimed by a dry-run
prerequisite assertion; the earlier cold run supplies the failure evidence.

The remaining 53 classes have now received an individual
[source/assertion/resource review](../development/ci-owner-review.md): 50 tooling
owners and three real application owners. Existing registrations and all test
methods remain. Ordinary tuning-tool/report-test and shared runtime probes now
use complete discovery rather than a filtered fixture. Unknown owners and
unresolved dependencies remain conservative fallbacks. Two actual equipment/model
validator CLI edges are explicitly registered; policy/fixture references can
still make other Python scopes full. The dirty migration change itself remains
a verification-control import requiring exhaustive qualification before merge.

## Proposed implementation sequence

### 1. Control admission before optimizing tests

The confirmed campaign/encounter classification and local prerequisite gaps are
fixed in the candidate, and the 53-class backlog is reviewed. Clean-environment qualification
and explicit pinned-interpreter bootstrap remain necessary; do not rely on test
order, a warm target or another worker's implicit build.

Both diagnostic drivers remain local-only. The automatic exhaustive schedule is
removed from core CI. The separate advisory workflow uses manual dispatch
with one bounded Ubuntu job, the existing pinned
cargo-deny action and root advisory policy. There is no warning-only override,
application build, test execution or qualification claim. This does not extend
advisory coverage to the independent 3D workspace. PR/main gates, manual full
runs, published-release triggers and conservative full-risk escalation remain.

The [periodic verification policy](../development/testing.md#periodic-verification-during-migration)
and its workflow tests now describe this migration boundary together. Complete
periodic tests remain local, recording actual host/profile coverage; hosted full
verification stays explicitly requested until a budgeted cadence is approved.
No new local automation or hosted run is started. Configuration tests check the
absence of hosted schedules, bounded blocking scan and retained full triggers. Real
child processes execute the advisory scope step for allowed and rejected events.
The existing planner still treats manual/release and unexpected events as full,
and advisory output cannot satisfy either required platform aggregate.

Readiness gating is implemented. Drafts receive scope, lint and dependency
feedback; eight core application job groups are excluded. Both required core
aggregates reject draft qualification before checkout. Independent 3D matrix
jobs use the same early guard so their existing check names fail explicitly;
this still allocates four short runner jobs, but starts no client build.
Ready/draft transitions trigger both workflows and existing concurrency cancels
superseded PR runs. `ready_for_review` restores the complete required plan.
Non-PR qualification is unchanged. Admission is covered by condition tests and
real Python child-process rejection/acceptance tests, not a hosted pilot.

Minor/patch Dependabot version updates are grouped per ecosystem; major and
security updates remain independent. Weekly cadence and open-PR caps are
unchanged. Batch ready-for-review checkpoints and keep security updates timely;
do not resolve incompatible updates by suppressing tests. Do not use skip-CI commit
messages as the resource-control strategy because required checks can remain pending.

### 2. Qualify once before merge, integrate cheaply afterward

This phase is implemented in the migration candidate, with complete required PR
coverage retained. [Proof reuse](../development/ci-proof-reuse.md) records the
separate authorities and independent assertions. Selected seals require the exact
contract/lane/test union, bound logs/results and actual compilers/configuration.
The bounded exact-tree fetcher checks the newest canonical run. Selected
results retain their coverage label and cannot chain reuse.

Main retains the original trigger. Selected application scopes run fresh Linux
integration. Pure tooling scopes run fresh pure tests with a no-build guard.
Core tooling and blocking dependency enforcement remain. Fork PRs verify freshly
without producing the new seals.

The new resolvers install and recheck the sealed numeric compiler release, commit
and Cargo identity; moving stable cannot silently change main's qualification.
Missing, stale, incomplete or incompatible proof, changed integration bases,
direct pushes and squash/rebase histories retain normal fresh coverage. Partial
attempts do not qualify the new proofs. Workflow/control changes still escalate
the import. Fresh workers derive Git selection without late proof queries.

A new repository has no qualified prior run history; importing source does not
import Actions proof. The local process/fixture tests do not measure hosted
acceptance or cost. After a separately authorized bounded pilot, measure proof
acceptance and fallback reasons for full and selected modes. Global
compiler updates and measured rollout remain separate changes.

### 3. Remove duplicate compilation and narrow real owners

[Hosted 3D smoke coverage](../development/3d-compilation.md) replaces the
four native lanes, separate proof jobs and compatibility job with one Linux
debug job. It builds the client and a local server, loads a game and checks
native input. The local debug and release verifier owns correctness of art,
geometry and gameplay. Hosted timing remains unmeasured.


### 4. Bound storage and make resource use visible

Retain small qualification receipts for the validated proof window plus a margin.
Ordinary binary artifacts already use one-day retention; keep that bound.
Independent 3D logs now use seven-day retention instead of the repository default.
Shorten other bulky successful logs only after confirming they are unnecessary to
proof validation. Keep failure evidence long enough to diagnose. Cache pinned
dependency environments with complete lock/hash/platform keys only when measured
restore cost beats reinstall; do not cache credentials or claim cache hits as tests.

Report actual runner-minutes by OS, setup/build/test/artifact time, retries,
superseded work and proof fallbacks after each accepted hosted checkpoint.
Use `scripts/ci_cost_report.py` on complete paginated all-attempt job exports;
do not run a new scheduled Actions workflow merely to poll CI cost. Configure an
Actions budget with stop-usage enabled where supported; alert-only budgets do
not cap usage. Billing settings are a separate maintainer decision.

## Acceptance and rollout

The candidate target is at least 50% lower combined PR-plus-main runner-minutes
for representative normal code changes, at least 80% lower main-side work for
eligible proven merges, and zero standalone hosted visual-review or measurement
workflows. These are
engineering targets, not promised savings; do not add overlapping estimates.
Measure comparable changes, compiler/images and warm/cold cache conditions,
including failures/cancellations, and report latency separately from total burden.

Implement admission, proof reuse, build consolidation and ownership refinement
as separate tested changes. Each needs positive/negative selection tests,
missing/stale-proof rejection, required-check behavior and real boundary tests
at the layers changed. Test draft-to-ready, direct-main, merge-base drift,
fork PRs, cancellation, unknown paths and partial reruns. A skipped or zero-test
lane must not satisfy a merge gate. Do not run duplicate hosted shadow matrices
to establish confidence; compare local/offline plans before one bounded hosted
pilot is explicitly authorized. Restore the last verified workflow revision on
failure and keep Actions disabled if no qualified configuration is available.

The maintainer authorized local implementation of this admission/cadence change.
Further qualification-policy reductions, new schedules, budget changes or Actions
re-enablement require a separate decision. The first import is a
workflow/control change and currently requires exhaustive hosted qualification
before any later merge; this local preparation does not supply that evidence.

## GitHub references

- [Actions terms](https://docs.github.com/en/site-policy/github-terms/github-terms-for-additional-products-and-features#actions)
- [Workflow concurrency and syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax)
- [Managing and skipping workflow runs](https://docs.github.com/en/actions/how-tos/manage-workflow-runs)
- [Dependency caching versus artifacts](https://docs.github.com/en/actions/concepts/workflows-and-actions/dependency-caching)
- [Artifact retention](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/remove-workflow-artifacts)
- [Budgets and stop-usage controls](https://docs.github.com/en/billing/how-tos/set-up-budgets)
- [PR event activity types](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows)
- [Dependabot update groups](https://docs.github.com/en/code-security/reference/supply-chain-security/dependabot-options-reference)
