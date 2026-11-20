# Action compositor

This plan composites everything a client shows as one *beat* into a single
tick-domain timeline. Moves, overlapping attacks and effects then play together
and continuously instead of one update at a time. The maintainer accepted the plan
and authorized the milestone, including the breaking protocol version increment, on
2026-11-20. Each phase still follows the [testing policy](../development/testing.md).
References change only when a phase implements them; until then the
[protocol](../reference/protocol.md) and [client](../architecture/clients.md)
references describe current behavior.

## Problem

The [3D client](../../crates/client-3d/README.md) draws each server update as it
arrives, spaced only by a fixed 90 ms connection pace:

- Another actor's move or attack plays when its message arrives, not over the
  ticks it occupied, so overlapping actions do not overlap on screen.
- Travel and click-to-move ease toward each new cell separately. The body surges
  and stops at every step.
- A projectile or ray draws one short bolt per adjacent cell pair. On a shallow
  diagonal the heading alternates between straight and diagonal steps, so the
  missile zigzags.

The text client already gathers a turn's updates before narrating them. Its
boundary logic lives in `client-text` only, so the 3D client cannot share it.

## Beats

A beat is defined by the client, not by a server input wait. It may span many
server input waits (*settles*) and includes actions the client supplies itself:
click-to-move paths, pursuit, repeated attacks, and parser commands that expand to
several actions. Each client supplies its own beat policy on a shared engine. A
beat ends when:

- **the plan ends:** the client's own plan finished, failed or was refused;
- **the client interrupts:** it judges that something needs the human, such as a
  newly noticed actor, damage, or a travel phase other than Active;
- **a decision is required:** the server needs something only the human can
  answer, such as a decision phase, a choice or menu, or loss of control;
- **a length cap is reached.** Capped chunks are presentation-continuous: the
  3D timeline plays across them without stopping, and narration may join them.

Client plans decide their next action from authoritative state, never from the
animated picture. The driver sends the next action as soon as the server is ready,
but pauses while the simulation is more than a bounded lead (about 1.5 s of
playback) ahead of the screen. Human input waits until the beat ends and playback
drains. A key fast-forwards playback.

## Disclosure amendments

The compositor needs to know when visible actions started. Today
[public plans](../reference/protocol.md#dungeon-combat) are the only way to learn
another actor's preparation, and idle, recovering and private work are
deliberately identical. That reference anticipates a later licensed public
provenance for recovery; these amendments are that licence. Each amendment is past
tense, applies only to actors visible at that boundary, and reveals categories
rather than numbers:

| Amendment | Disclosed | Never disclosed |
| --- | --- | --- |
| **P1 Wind-up posture** | When a visible actor begins public ability preparation: a posture category (melee, ranged, casting, other) at its start tick | Ability identity unless already licensed, aim, remaining work, strike totals |
| **P2 Follow-through** | After a visible public action (step, strike, cast): that a follow-through began, by category | End or ready tick, remaining recovery |
| **P3 Item handling** | A visible actor drinking, reading, zapping, eating, donning, doffing or wielding, with the item as the observer perceives it (unidentified appearance), as NetHack does | Identity the observer has not learned, charges, effect primitives |

Unchanged:

- Idle, recovery after no public action, and other private work still look
  identical.
- Another actor's ready tick, remaining work and future strike count are never
  sent.
- Their recovery end is learned only when they next act, exactly as by watching.
- Exact own timing remains limited to the owner, which already receives it through
  outcome status facts.

Using observed item handling as an identification source is a separate future
rules change. P3 itself teaches nothing.

## Protocol and server (phase 1)

Ordered outcomes gain one fact, `activity`, for each disclosed actor activity at
its committing boundary:

- **Required fields:** `actor` (an observer-scoped target), `started_at`, `kind`
  and `phase`, plus a nullable `ready_at` that is present only for the observer's
  own actions.
- **Kinds:**
  - a move with nullable observer-frame `from` and `to` and a diagonal flag;
  - a P1 posture;
  - a P2 follow-through;
  - P3 item handling;
  - a door;
  - a pickup;
  - wait (own only);
  - ability (own, or licensed).
- **Phases:** committed, preparing (remaining work own only), ended, and the own-only
  suspended and abandoned.
- **Positions and strikes:** positions use the same observation frame as effect
  segments. Strikes remain existing combat events, which clients link to an
  activity by caster and tick.
- **Validation:** rejects a future start, self-only fields on other actors, and
  out-of-range positions.

The server derives the fact from the settled action and its post-action readiness.
The scheduler's next decision tick is not the actor's ready tick. The fact is
built in a dedicated outcome adapter beside the existing work-change facts, so
role filtering and the disclosure rules above apply before capture.
`PROTOCOL_VERSION` advances by one. Every client that matches facts
exhaustively handles the new variant, and fixtures are regenerated.

## Shared compositor (phase 2)

`tor-client-common` gains a renderer-neutral compositor:

- **Settle boundary:** the text client's current turn-settle decision (`Until`,
  `Settled` and admission boundaries) moves into the shared crate unchanged,
  behind characterization tests recorded first. Then the text client gets its own
  beat policy, so multi-action commands close as one narrated beat. That change is
  a separate commit with its own tests.
- **Batch model:** per-actor spans with commit tick, linked strike ticks and an
  end. The end is exact for the owner, inferred from the actor's next activity, or
  open with a cap from its last observed step interval. Effect flights and
  reactions sit alongside the spans.
- **Streaming:** the API accepts messages in order, reports the latest stable
  cursor tick and drains closed timeline items. Positions are converted per
  observation frame, because the observer may move during a beat. A replacement
  snapshot or resynchronization flushes the beat.

## 3D playback (phase 3)

- **Clock:** a renderer-free tick clock maps ticks to milliseconds at a fixed
  rate. Intervals with nothing visible, and held stances, compress to a cap.
  Playback starts after a lookahead of about 120 ms, too short to notice.
- **Moves:** a move fills its commit-to-ready span. Chained spans make one
  continuous walk, and the body trails its authoritative cell by at most one step.
  The previous exponential ease remains only for snaps such as resynchronization,
  teleport and falls.
- **Attacks:** these are phase-mapped. The wind-up fills the posture start to the
  first strike; when the start is unknown, a cosmetic wind-up ends on the strike.
  Each impact frame lands on its strike tick. The follow-through and a compressible
  held stance fill the recovery.
- **Effects:** these use only information already disclosed in the effect event
  and the existing ability and death events, and receive nothing from the new
  activity fact.
  - Each visible segment gets one straight missile fitted through that segment's
    disclosed cells, from the first to the last, at a fixed client visual speed.
  - It never extrapolates past a segment's ends and never flies toward an
    undisclosed origin, target or cell.
  - A hidden gap is not drawn. The missile reappears after a delay derived only
    from the two disclosed endpoints.
  - Reactions wait for arrival only for targets named by disclosed events whose
    cell lies on a disclosed segment. Otherwise they play at the effect tick.
- **Pacing and pursuit:** the 3D connection pace becomes zero (the text client
  keeps its own). Pursuit becomes a client plan under the bounded-lead rule.

## Verification

| Contract | Owner |
|---|---|
| Wire round-trip, rejection and fixtures | `crates/protocol/tests/it` |
| Disclosure:<br>• hidden actors emit nothing<br>• leaving sight yields a null endpoint<br>• no wait or ready tick for others<br>• private work is silent<br>• posture only for public preparation<br>• follow-through without an end<br>• unidentified item appearance<br>• exact own ready tick<br>Witnesses: an ordinary actor, a creature, and travel across a trap. | server adapter tests and a Python Service contract |
| Compositor:<br>• beats across settles with client actions<br>• each end reason<br>• joined capped chunks<br>• the bounded-lead pause<br>• overlapping actions<br>• travel continuity<br>• multi-strike<br>• interrupted own preparation<br>• segmented projectiles<br>• frame change<br>• resynchronization | `crates/client-common/tests/it` |
| Text-client parity, then text beat policy | characterization tests plus the existing turn and strike-sequence owners |
| 3D:<br>• clock and gap caps<br>• span sampling<br>• disclosed-only straight flight<br>• reaction delay<br>• input gating and fast-forward | `crates/client-3d/model/tests/it` |

Actual-process checks use the text turns, the headless effects test and the 3D
scenario review, followed by a manual launcher run covering:
- long travel;
- overlapping melee;
- a diagonal ray or arrow;
- a monster drinking a potion.

The 3D workspace keeps its [independent verification procedure](../development/testing.md#independent-3d-client).

## Risks

- The text settle migration is the riskiest step, so it stays behind
  characterization tests in its own commit.
- Long fights or travel can build a playback backlog. Playback speeds up past a
  threshold, and fast-forward is always available.
- An open span delays the lookahead only up to its cap.
- ASCII effect playback may adopt the shared timeline later. It is not part of
  this plan.
