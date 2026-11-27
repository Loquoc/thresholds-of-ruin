# Command retry and save recovery pilot

A deliberately small, local design check. It models journal admission for an
ordinary pickup-shaped command, reconnect, asynchronous storage, an explicit
save barrier, and crash recovery. All actions can interleave. The model has
no dependency on the server binaries and is not a CI gate.

## Run

Use Python 3, Java, and the upstream
[TLC 2.19 jar from TLA+ tools v1.7.4](https://github.com/tlaplus/tlaplus/releases/tag/v1.7.4).
Check the jar against the release checksum; do not commit it.

```sh
python3 specs/tla/retry-durability/check.py /path/to/tla2tools.jar --sanity
```

The baseline explores two request IDs, two opaque item choices, and up to
three attachments for one already-authorized player, actor, and branch.
The extra checks deliberately vary only temporary copies of the model:
acknowledging before persistence, skipping receipt lookup, and admitting old
input must each violate the expected invariant. Two separate witness checks
confirm that the model reaches a durable retry on a later attachment and a
completed save while a later entry is still volatile. Deadlock checking is
off because an exhausted attachment bound is an intended terminal test
state. The experiment makes no liveness claim.

## What corresponds to the game

- **Admission and retry.** A fresh request must use the current attachment
  basis. An existing same-ID receipt is resolved first, returning the
  original admission without creating another. Changing the ordinary
  command's item is a conflict. The old attachment basis does not itself
  invalidate a receipt hit. See
  [command processing](../../../docs/reference/protocol-lifecycle.md#command-processing-order-matters)
  and `Engine::retry_matching`.
- **Storage and save.** Admissions and their receipts share a durable
  prefix. A save captures the prefix already accepted when it arrives
  and can reply only after that prefix is durable; it makes no promise
  for later entries. A crash discards the remaining volatile tail.
  See [persistence and recovery](../../../docs/architecture/persistence.md).
- **Grounding in code.** The existing
  `durable_wire_retry_resolves_before_a_taken_item_or_revision_is_checked`
  server test checks a real reopened engine and exact receipt identity.
  The existing
  `test_acknowledged_tail_is_lost_but_explicitly_saved_prefix_resumes`
  process test checks the crash/prefix boundary with a server and client.
  The Rust process suite already checks a resolved command's exact reply
  after restart. The companion suspended-queue case checks that the retry
  reports the original admission and current reply context even while a
  new command is refused. Neither retry should advance the simulation.

The model checks admission, not action execution. It assumes the documented
atomic storage prefix; it does not establish that SQLite or Rust implements
that assumption. It omits permissions, branch rewinds, deferred-command
admission, timeout responses, checkpoints, and wire encoding. Keep real
code/process tests as the source of implementation assurance.
