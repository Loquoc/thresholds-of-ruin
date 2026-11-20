# GitHub Actions usage review

Reviewed November 11, 2026, for the account migration. GitHub limits hosted
runner activity to work associated with the software project and also considers
whether runner burden is proportionate to its benefit. Its acceptable use policy
addresses bulk automation and inauthentic activity driven by rewards. The rules
below are this repository's conservative operating policy. GitHub determines
enforcement. [Additional Product Terms](https://docs.github.com/en/site-policy/github-terms/github-terms-for-additional-products-and-features#actions),
[Acceptable Use Policies](https://docs.github.com/en/site-policy/acceptable-use-policies/github-acceptable-use-policies).

| Workflow or jobs | Project purpose and reviewed boundary |
| --- | --- |
| Core scope and tooling | Read the checked-out source, classify the diff, check local links, and lint Python and workflow files. Prose changes can avoid application builds. |
| Core dependency jobs | Scan this workspace's dependency graph for advisories, bans, licenses and sources. The advisory result is warning-only on PRs and blocking on main and explicit full runs. |
| Full profiles and Windows build/test shards | Compile and verify this project's supported OS/profile combinations when risk, a published release, or explicit dispatch calls for full coverage. Application binaries move between jobs in the same run to avoid duplicate compilation. |
| Selected Rust, Linux, Windows and release jobs | Run reviewed contracts for the changed source. Application builds and release tests depend on the selected plan. Draft PRs skip application jobs. |
| 3D compatibility | Build the repository's client and server and run one Linux debug launch-and-input smoke test with a local display and loopback server. Art review, model rendering studies and the full native verifier stay local. |
| Post-merge integration and qualification | Read this repository's GitHub PR, run and artifact metadata to qualify exact-tree reuse. Run fresh integration when reusing proof. Missing or invalid evidence falls back to fresh verification. No cross-project harvesting. |
| Manual dependency advisory scan | One read-only Ubuntu job with a ten-minute limit, checking the root workspace. The recurring trigger was removed. Dependency checks also remain in core CI, and Dependabot's dependency update configuration is separate from Actions. |

The workflow inventory is `ci.yml` and `dependency-advisories.yml`. Core CI runs
on main pushes, PR readiness and update events, explicit dispatch and published
releases. Neither workflow runs on a timer. Both cancel superseded runs and use
hosted runners with job timeouts. A full plan can admit 14 application jobs:
seven core profile jobs, two Windows binary producers, four Windows test shards,
and one 3D smoke job. The longest configured individual timeout is 75 minutes.
These are limits, not measured usage. Keep the selected plan, bounded fixtures
and reusable build outputs, and use manual full
runs when they answer a concrete release or regression question. Hosted timings
have yet to be measured on the destination repository.

## Network and artifacts

- Checkout and GitHub API reads support this repository's source and qualification.
  Checkout disables persisted credentials. Workflow permissions are read-only,
  and the GitHub token is scoped to jobs that read repository or run evidence.
- Cargo, Rustup, pip and apt retrieve build and test prerequisites. Cargo-deny
  consults dependency and advisory metadata for this project's manifest. These
  contacts are inputs to project verification. The pinned cargo-deny action uses
  a digest-pinned Rust/Alpine base image, Alpine package mirrors and the
  cargo-deny GitHub release download. Its optional credential inputs are unset.
  The pinned Rust cache action uses GitHub's cache provider. Review upstream
  behavior when either pin changes.
- GitHub artifacts hold verification logs, receipts and binaries needed by this
  project. Plans and seals expire after two days, transported binaries after one
  day, and diagnostic logs after seven or fourteen days. They serve test handoff
  and investigation. Release uploads and external publishing are outside these jobs.
- Test scenarios and process fixtures exercise this project. The full suite still
  has bounded regression witnesses that invoke named benchmark binaries. They
  assert properties of the project's reports and behavior. Large comparative
  runs, long retention studies, arena searches, and worn-equipment render
  reviews stay on the developer machine. The retired diagnostic workflows must
  remain out of hosted CI.

No reviewed job implements third-party website automation, reward collection,
engagement, mining, or general-purpose compute. No job exposes a generic command
input or a long-running service. A workflow whose only observable outcome is an
external interaction or a data/render batch belongs outside Actions under this
repository's rule, even if it could be made cheap.

## Before changing or running Actions

1. Identify the source, release or repository dependency change being verified and
   the assertion the job produces. Read the current GitHub terms linked above.
2. Follow the workflow into local scripts, selected tests, build hooks, pinned
   actions and dependencies. Account for destinations, credentials, triggers,
   timeouts, maximum fan-out, artifacts and retention. Keep external access to
   necessary project dependencies and repository services. Use a local fixture
   instead of a live third-party service for behavior tests.
3. Update this review for any changed behavior. After human review, update the
   affected SHA-256 entries in `scripts/testdata/ci-usage-review.json` and run
   `python scripts/ci_usage_review.py`. The checker reports new, missing or
   changed reviewed files. It runs locally and before CI planning or a manual
   advisory scan. A failed or absent review stops core dependency and build
   admission even when scope would otherwise fall back to exhaustive CI. Do not
   refresh hashes simply to make a check pass.
4. For a new workload or higher frequency, justify its cost against the project
   benefit and use the smallest representative test. Check actual runner minutes
   and artifact growth when evaluating a hosted rollout.

The hash check detects drift in the workflow inventory, dependency configuration
and listed CI entry points. It does not inspect transitive execution or enforce
network access. Review the full diff, including changed tests and build hooks,
even if the checker passes. During migration, Actions remains disabled and
publication requires the maintainer's authorization.

## November 14 dependency refresh

The maintainer explicitly approved Ruff 0.17.3 to 0.17.4 and SQLAlchemy 2.1.6
to 2.1.8, including refreshed review hashes after the sampler consistency fix.
These patch updates retain the existing lint and bounded arena-test purposes,
package registries, runner types, permissions, triggers and artifact retention.
SQLAlchemy remains wheel-only and hash-pinned. Its strict sampler allowlist is
updated together with rejection tests for the superseded environment and history;
no compatibility fallback or test gate is relaxed. The two requirements hashes
record these reviewed inputs. Hosted CI still qualifies the final PR head.

## November 15 Linux release verification budget

PR #11 run 43697031043 exhausted the 40-minute Ubuntu release Python job
limit twice without a failed test: 659 and 681 of the same 770 selected tests
completed successfully. Build time was 332 and 329 seconds. The ordinary
room-topology acceptance witness completed in 1,552 and 1,557 seconds. The
same complete inventory passed in 2,088 seconds including build in run
43686561081, leaving too little runner-variance headroom at the former limit.

The maintainer-approved correction limits this single unsharded Ubuntu release Python job
to 60 minutes and its verifier step to 55 minutes, reserving time for setup and
evidence upload. Other job limits, all test-level action/time/output budgets,
selection, two test workers, receipt completeness, and required gates stay
unchanged. No jobs, triggers, permissions, network destinations, artifact
retention, display tests, or workload are added. The longest configured job
remains the existing 75-minute Windows release Rust owner. This is bounded
headroom for ordinary project acceptance, not permission for extra diagnostics.
The maintainer explicitly approved these two timeout changes and the matching
CI review hash update on November 15.

## November 16 diagnostic-validator ownership review

The two new model report validator test classes are ordinary pure-Python
repository correctness tests, not hosted model generation or performance work.
Reviewed their assertions, reporter imports and shared validator: synthetic rows
only, with no native process, rendering, benchmark, network or artifact producer.
Explicit class registration repairs complete-inventory planner failures while
preserving unknown-owner escalation and full verification for control changes.
No workflow commands, runners, external dependencies/destinations, permissions,
triggers, caches, retention or budgets change. Updated only the reviewed
`ci_test_contracts.py` hash after this source review. The cord correctness tests
exercise bounded geometry/animation contracts; exploratory timing, geometry
exports and parameter sweeps remain ignored/local, not new hosted workloads.

## November 16 bot process selection review

The verifier now selects `test_rogue_bot_process` for changes to
`tor-client-bot`. The process tests use a local Python extension and a
loopback server. The maintainer reviewed this selection change. It adds
no workflow triggers, runner types, network destinations, or uploads.

The maintainer also reviewed three planner registrations in
`scripts/ci_test_contracts.py`. The bot session tests exercise a real local
application. The policy and cohort summary suites use Python fixtures.
The registrations preserve the existing discovery and fallback rules.

## November 17 Luau scripting runtime dependency

The maintainer explicitly authorized scenario scripting with Luau, a
compatibility-breaking package format bump, and the runtime dependency. The new
workspace crate `tor-scripting` depends on `mlua` 0.12.2. It uses only the
`luau` and `vendored` features, so `mlua-sys` builds the pinned
`luau0-src` C++ sources through `cc` during the ordinary Cargo build.

All new crates come from crates.io under licenses already allowed by
`deny.toml`, which is unchanged:

- MIT: `mlua`, `mlua-sys`, `luau0-src`, `lua-src`, `luajit-src`, `which`,
  `redox_syscall`.
- MIT or Apache-2.0: `parking_lot`, `lock_api`, `bstr`, and the small support
  crates.

`lua-src` and `luajit-src` are optional backends. They appear in the lock file
and the all-features license graph, but they are not built.

The build script compiles bundled sources only. It downloads nothing at build
time and reaches no network destination beyond the existing crates.io registry.
The hosted Windows (MSVC) and Ubuntu images already provide the needed C++
compilers, so no workflow step, runner type, trigger, permission, cache,
artifact or retention changes.

The added hosted cost is one C++ library compile per cold Rust build. Its tests
are bounded repository correctness tests: the sandbox, budgets, determinism
goldens, and scenario hook fixtures. They are not benchmarks or generated
workloads. No reviewed hash changes for this dependency.

## November 18 Ruff dependency refresh

The maintainer authorized handling the current Dependabot dependency updates,
including fixes and publication, with merging conditional on passing CI. PR #24
updates only the pinned Ruff package from 0.17.4 to 0.17.5. Reviewed the actual
requirements diff, the repository's Ruff configuration, the lint job and its
installation command against the current GitHub terms linked above. Ruff still
checks only this repository's Python tooling with the same non-preview bug-finding
rules; no autofix, new workload or external interaction is enabled. Installation
uses the existing pip package registry, and actionlint remains unchanged.

The existing Ubuntu lint job, ten-minute timeout, read-only permissions,
checkout behavior, workflow triggers, application gates, runner fan-out and
artifact retention are unchanged. The original PR run already passed the
repository tooling lint job; admission stopped because the reviewed requirements
hash had not yet been updated. This review records the new requirements input,
not a gate exception. Required hosted CI must still qualify the final head.

## November 19 outcome-script test registration

Scripted outcomes register one planner owner in `scripts/ci_test_contracts.py`:
`test_outcome_scripts` (class `OutcomeScripts`). Reviewed its source: it reads
the checked-in scenario packages and the canonical outcome scripts, and writes
one generated script into a temporary directory. It starts no process, server,
renderer or benchmark, reaches no network destination and produces no
artifact. No workflow commands, runners, triggers, permissions, dependencies,
caches, retention or budgets change. Updated only the reviewed
`ci_test_contracts.py` hash after this source review.

## Python SDK and bot launch selection

Changes to the five reviewed Python SDK modules select the tests of the
package's consumers. The evaluator follows ordinary imports as well as the
cohort runner's file copies and the process fixtures that launch the bot.
Replaying PR #35 selected the Linux debug build, Linux and Windows debug
Python tests, and one Linux release Python test. Unknown SDK files and
unexplained runtime references still require exhaustive verification.
This review changes test selection within existing jobs and budgets.
