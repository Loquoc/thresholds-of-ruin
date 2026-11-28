# Compositor beat ordering experiment

This model checks the decision boundary shared by settlement, beat composition,
and the ordinary 3D movement/playback driver. It does not model rendering
quality, graphical time, the wire decoder, or animation interpolation.

Run with Python 3, Java, and the checksum-verified
[TLC 2.19 jar from TLA+ tools v1.7.4](https://github.com/tlaplus/tlaplus/releases/tag/v1.7.4):

```sh
python3 specs/tla/compositor-beat/check.py /path/to/tla2tools.jar
```

## What the model asks

- The next automated step requires the current request's receipt, outcome,
  input permission, and usable picture. Acknowledgement or fast-forward
  alone cannot authorize a step. With permission and current state, the
  plan may take the next step while some playback is still queued, but
  only within its lead limit.
- A replacement snapshot ends the old plan and discards its queued playback.
- Manual gameplay input is eligible after the plan ends only when
  authority is current and playback has drained. Under the stated progress
  assumptions, the modeled client eventually reaches that boundary.

The model uses one initially controlled, ordinary movement plan, at most
two or three sends and one scope replacement. A completed outcome supplies
two synthetic playback units; zero/one unit of lead are test bounds, not a
claim about the real seconds threshold. Receipt and outcome may arrive in
either order; permission and usable picture are separate post-outcome
events. Other actions, hostile waiting, rejection, lost control, timeout
recovery, and a second plan are out of scope. Replacement represents a
successfully installed snapshot that already has a usable view and permits
input. Later messages for an abandoned scope belong to stream validation,
not this model. Fast-forward abstracts marking playback due and then
draining it; even that combined event never grants authority.

The temporal property assumes progressing deliveries, playback, and
execution of enabled finite steps. It is not a guarantee if the server,
front end, or player stops making progress.

## What ran

| Bound | Distinct states |
|---|---:|
| Two steps, one unit lead | 208 |
| Three steps, one unit lead | 332 |
| Three steps, no lead | 225 |

All bounded runs satisfied the invariants and progress property. Witnesses
reach both receipt/outcome orders, skipping before permission, continuing
within the lead before full drain, and returning with and without a replacement.
Five deliberately broken variants were caught: continuing after just the
receipt, continuing from fast-forward alone, retaining the old plan or frames
after replacement, and returning human control before playback drains.

Implementation anchors: shared [settlement](../../../crates/client-common/src/compositor/settle.rs)
and [beat rules](../../../crates/client-common/src/compositor/beat.rs),
3D [input and dispatch](../../../crates/client-3d/src/lib.rs),
[playback](../../../crates/client-3d/src/playback.rs) and
[scene realignment](../../../crates/client-3d/src/scene.rs). The Rust fixture
`replacement_after_outcome_without_permissions_ends_the_pending_settle_and_beat`
drives the settlement and compositor together on a replacement boundary.
It is not a native 3D playback test. The model is optional and not a CI gate.
