# Exact-tree selected proof reuse

The migration candidate extends the existing full-PR proof mechanism through
[`ci_contract_reuse.py`](../../scripts/ci_contract_reuse.py). Actions remains
disabled and the destination repository remains empty. Local tests validate the
contracts; they do not establish hosted qualification or measured savings.
The [testing policy](testing.md#exact-tree-post-merge-assurance) owns merge gates.

## Coverage authorities

| Authority | Producer | Fresh main work |
| --- | --- | --- |
| `prior-exact-tree-full-plus-fresh-main` | Existing fresh exhaustive core PR seal | Existing Linux application/codecs, pure tests and two persistence witnesses; tooling and blocking dependencies |
| `prior-exact-tree-selected-plus-fresh-main` | Fresh selected PR seal after both core aggregates and every required lane pass | The same Linux integration for application scopes; pure tooling scopes run all current pure tests under the no-build guard without compiling applications. Tooling and blocking dependencies remain |

Selected evidence never becomes an exhaustive seal. Core compatibility compilation
never becomes native 3D acceptance. Reused main results cannot produce a new PR
seal or chain coverage into another tree. Manual and release qualification retain
their existing fresh coverage. A reused result is explicitly labeled in the plan
and required-check summary; it is not a fresh Windows/release or full-matrix run.

## Origin, scope and compiler binding

The shared bounded resolver accepts only an ordinary two-parent merge to `main`,
one same-repository merged PR, and its newest canonical successful workflow run.
There is no historical-success search. It checks complete bounded job/artifact
metadata, exact run and attempt, archive digest and safe single-file contents,
tested Git object, both parents, exact source tree, workflow blob, policy,
inventory and integration witnesses. Proof must finish before merge and be no
older than 24 hours. Small attempt-specific seals are retained for two days.

The new proof kinds require one execution attempt for every required role.
Partial reruns, missing/duplicate lanes or seals, forks, changed bases, direct
pushes, squash/rebase histories, expired artifacts, unknown job roles and compiler
or configuration mismatches keep normal fresh qualification. Unsupported metadata
is not guessed into a narrower plan. This is intentionally stricter than the
existing explicitly validated full-shard cohort transport.

Selected proofs additionally require the same independently recomputed contract,
lane, profile and exact test-ID selection on PR and main. Every lane records its
actual Rust/Cargo identity and allowed build configuration. The seal independently
reads bound stage logs, Python lifecycle/results, no-build audits and application
manifests through the existing selected validators. Fresh workers recompute normal
Git selection without querying proof availability: a late proof cannot change an
already admitted fresh lane or its receipt hash.

All compiler roles must agree on release, full compiler commit and Cargo identity.
After validating proof, the new resolvers install that exact numeric release and
recheck its identity. They do not resolve a moving `stable` again on main.
Unreviewed compiler, target/linker, profile and external Cargo overrides are
rejected. PR jobs still resolve stable independently; disagreement prevents reuse.
A repository-wide compiler-update policy remains a separate decision.

## 3D smoke coverage

The selected and full core proof paths include the
[3D smoke check](3d-compilation.md) when their plan requires it. The separate
native matrix and native proof path were retired when CI moved to smoke coverage.

## Validation and limits

`test_ci_contract_reuse.ContractReuseTests` owns the new proof, seal, workflow and
integration-accounting contracts. Its resource review permits source/data fixtures,
compiler metadata, isolated API transport children and actual guarded Python test
workers; it does not run Rust/native applications. Existing full-proof and selected
validators retain their independent positive/negative witnesses. Tests mutate
origin, tree/base, newest-run status, artifact integrity, attempt, lane union,
compiler/configuration, actual test outcomes, canonical commands and fresh-job status.

Local process tests exercise real bounded subprocess transport with an isolated
fixture service, CLI fallback/rejection, actual workflow guards and a real selected
tooling worker with empty application discovery. Source/lock authority in that
worker test is a fixture; these receipts are not hosted qualification. The hosted smoke path still needs a hosted pilot.
This control-changing import still needs exhaustive hosted qualification before
merge, after separately authorized upload and Actions re-enablement. No source,
artifact, release or credentials are uploaded by local validation.
