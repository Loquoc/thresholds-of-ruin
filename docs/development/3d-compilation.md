# Hosted 3D smoke check

Core CI runs one Linux debug job when a plan selects 3D compatibility. The job
builds the locked server and client binaries, starts a fresh local two-room game,
waits for a connected frame, sends a native key, and checks that Escape exits
cleanly. Xvfb provides the display. A failed build, launch, input check, or exit
fails the job.

Selected and exhaustive core plans use the same commands. A 3D-only source
change can run the smoke check with the ordinary tooling jobs. Changes to shared
core code retain their existing core tests. The standalone hosted 3D workflow
and its separate proof chain are retired. Exact-tree full and selected core
proof can retain their own completed smoke coverage under the normal reuse rules.

Use the [3D client guide](../../crates/client-3d/README.md#independent-maintenance) for the
local debug and release verifier. That verifier owns model geometry, art,
scenario, and gameplay correctness. Its assertions and native timeouts are
independent of the hosted smoke check.

The smoke entry point also works locally from the repository root with the native
display and Rust prerequisites installed.

```sh
python scripts/ci_selected.py smoke-3d --output NEW_OUTPUT_DIRECTORY
```

Use a new output directory under `.local/` for each run. It contains the build logs and the
single test's outcome. CI keeps its logs for seven days.
