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
