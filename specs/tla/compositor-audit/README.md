# Extended compositor audit

The [compositor beat pilot](../compositor-beat/README.md) checked the overall
handoff. These three smaller models examine the actual boundaries separately.
Keep the models opt-in; their state counts and counterexamples are evidence
about abstractions, not a proof of Rust or a substitute for native tests.

```sh
python3 specs/tla/compositor-audit/check.py /path/to/tla2tools.jar --traces .local/compositor-audit-traces
```

Use the same pinned, checksum-verified TLC 2.19 jar as the pilot. Drop
`--traces` to omit the per-case logs. A deliberately false reachability
invariant must produce the specific expected counterexample. A deliberately
broken model must violate the named correctness invariant. An arbitrary TLC
failure is not success.

| Model | Default distinct states | Wider run |
|---|---:|---:|
| Settlement, one fresh admission | 140 | Terminal-receipt sensitivity below |
| Presentation queue, three/four views | 211 | 967 |
| Timeline, four/five events | 249,135 | 3,477,329 |

All baseline and wider normal runs passed. Eleven reachability cases,
sixteen deliberate faults and the separate terminal-receipt sensitivity
produced the expected named counterexamples.

## 1. Settlement: stopped work gets its own permissions

**Source boundary:** [settle](../../../crates/client-common/src/compositor/settle.rs)
and [admission](../../../crates/client-common/src/compositor/admission.rs).
This checks an ordinary fresh queued command, one in-progress phase, two
terminal outcomes, an optional unrelated update or older permission, and the
ways the client can give up on that wait. It distinguishes the input grant
after Started from the grant after actual resolution or failure. Yield while
the tracked work remains queued gives no new action permission. Control loss
and a replacement end the old wait; a deadline is not proof of execution.

TLC passes the bounded model. The controls catch reusing Started's permission,
treating earlier permissions or another intention's failure as authoritative,
returning a normal yield after known failure, or treating a replacement as a
success. The added Rust test combines Started permission, failure, and an
intervening wait: it stays unsettled until the later permission and then
returns the specific failure.

**Scope-sensitive finding.** If the fresh-command source contract is relaxed
and a receipt can first arrive after a Failed status, the modeled boundary
can classify that failure as Done. This is consistent with the Rust split:
the failure status is associated only after the receipt identifies the
entry, and the receipt's nonqueued phase skips the new-admission wait. It is
not a fresh-gameplay bug report. Fresh gameplay admits and acknowledges
before the server independently executes; the action driver does not
replay a request using a recovered receipt. If a later client routes such
recoveries through this same settle API, explicitly map terminal receipt
phases and test that path first. This warning does not apply merely because
an ordinary receipt may follow picture or readiness updates from the
admitting transaction.

## 2. Presentation queue: fold views, not authority

**Source boundary:** [3D worker/window bridge](../../../crates/client-3d/src/net.rs),
especially `frame_events`, `gather`, `mergeable` and delivery of `Event::Ready`.
The model enumerates bursts of up to three or four views with at most one
increase of presentation scope and timeline generation, same- or changed-frame
views, one capture reservation, and an ordered readiness marker. The newest
picture wins within a fold; earlier view contributions remain in the same
order. No Ready marker may become effective while earlier view content is
still gathered. Screenshot capture freezes consumption, not the producer's
ability to enqueue up to the abstract bound. Under progressing delivery and
release, the modeled marker eventually reaches the scene.

The controls catch early Ready on read-ahead, loss of earlier contributions,
folding across a scope/generation/own-step boundary or consuming during a
capture. This abstraction does not compare exact vector lengths, durations,
GPU capture sequencing, thread scheduling, or revision checks on submission.
The native unit tests in the source cover view/Ready ordering, own-step
gathering, capture blocking and bounded render backpressure independently.

## 3. Timeline: prove the provenance the data can support

**Source boundary:** [Timeline](../../../crates/client-common/src/compositor/timeline.rs)
and [the server's activity producer](../../../crates/server/src/outcome_activity.rs).
Two actors share a three-tick window; disclosed starts for any one actor are
nondecreasing, as the host emits each activity from its committing action.
The model allows delayed disclosure relative to the playhead and simultaneous
activities by different actors. It checks that one actor's activity doesn't
close another's span, death bounds an open span, each record retains its
disclosure frame and scope, and a linked reaction chooses the newest preceding
flight with the same tick and caster. Replacement removes earlier scope data.

The controls catch closing the wrong span, forgetting to close on death,
linking by caster or tick alone, and retaining old reactions on a reset.
The extended Rust test inserts another actor's same-tick flight before the
reaction and still expects the original actor's flight.

**Limit that matters:** “newest preceding flight at the same caster and tick”
is not a unique causal link. A reachable model witness has two eligible
flights; the timeline deliberately chooses the last one. The check does not
assert that the current server publishes that ambiguous combination in play.
Production clients currently do not use `Timeline::flights()` or
`Timeline::reactions()` to drive 3D impact timing. If that changes, require a
disclosed effect/strike correlation key or specify and test a defensible
ordering rule before relying on the link.

## Scope and next work

This audit addresses settlement, queued view delivery and timeline structure.
It does not validate FPS, animation interpolation, actual network timeouts,
unbounded queues, all travel/refusal modes, or the full renderer. The most
valuable next cross-layer test would run a real 3D movement plan through a
replacement while playback is queued and verify that no old step resumes
and that visible input is restored on the new scene. The existing shared
Rust replacement test checks the boundary beneath that native scenario.

Local implementation checks: 12 shared compositor unit tests and nine
integration tests pass. As negative controls against Rust itself, the
failure/yield test detects removal of the waiting guard and the flight test
detects removal of the caster match. Both code paths were restored afterward.
A separate previously built 3D integration preview
also passed 16 window/worker and 27 playback unit tests; this is supporting
evidence, not a freshly linked build of this exact audit tree or a native
GUI/real-server playback run.
