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
