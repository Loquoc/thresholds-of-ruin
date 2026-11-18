# Python bot API and Rogue-O-Matic implementation plan

Proposed follow-up to PR #27, based on merged main at `496de9a8`. The first
checkpoint is a fresh campaign baseline and one recorded stair crossing.

## Goals and ownership

A Python author should be able to read a policy and explain what the player
saw, which action it chose, and what happened next. The Python client owns
convenient access to player-visible facts, choices and observed outcomes.
Reuse common-client rules for shared geometry and observations. The server
owns action resolution. Rogue-O-Matic owns tactical preferences, learned danger,
food budgets and the decision to return. Give uncertainty an explicit
representation whenever the player's evidence permits multiple outcomes.

The second goal is repeated retrieval of the Amulet and a live return to the
surface in fresh games of bundled `rogue`. The enemy-free fixture already
verifies the complete route. Historical bundled runs include acquisition and
a return to floor 2 before starvation. Use those failures to generate hypotheses
and measure the merged policy afresh. Consult the [API reference](../reference/bot-client.md)
for implemented behavior and the [acceptance record](../development/rogue-o-matic-parity.md)
for revision-specific results.

## Implementation and verification sequence

1. **Measure the baseline.** Build the wheel and server from the same commit.
   Run the full-depth fixture and fresh bundled development seeds 42, 47 and 49
   with the [cohort runner](../../scripts/rogue_o_matic_cohort.py). Pin the
   revisions, actor, budget and timeout. Reserve a separate held-out seed set
   before tuning. Record victories, deepest floor, acquisition, nearest approach
   to the surface after acquisition, actions and terminal cause. Keep the
   decision trace. For each failure identify the earliest materially harmful
   decision the evidence explains and record any gap in that evidence.
2. **Trace one stair crossing.** Connect a submission to its receipt, latest
   observed activity phase and subsequent player-visible evidence, including
   the origin and current cell when disclosed. Migrate the stair bookkeeping
   in [Adventurer](../../scripts/rogue_player.py). Exercise an ordinary
   descent, an adjacent landing beside an occupied endpoint, a rejected move
   and an ambiguous observation. Use a real server and client for the movement
   cases. Verify that established arrival switches maps exactly once, while
   rejection or uncertain arrival retains the current map. Save a crossing for
   the replay checkpoint.
3. **Reproduce decisions offline.** Capture any lazily fetched chart or
   projectile choices the decision consumed. Pair the recording with a separate
   fixture for the policy's starting memory. Expose the terminal observation
   and newly witnessed events through the ordinary decision loop. Supply a
   structured action description for traces. Replay the crossing, a shot that
   required ammunition selection and a terminal observation after the last
   budgeted action. Compare actions, destinations or targets, reasons, updated
   policy memory and terminal results with their live counterparts.
4. **Connect the visible scene.** Associate a visible ground item with its
   observed cell. Expose contact and body-to-body clearance for candidate
   moves. Specify coordinate frames, diagonal corners, doors and unseen space.
   Keep remembered sightings distinct from live interaction targets. Migrate
   food pursuit, retreat and routing to remembered stairs. Compare recorded
   decisions and exercise a multi-cell actor whose origin gives a different
   distance, a blocked diagonal, and a remembered meal absent on revisiting.
5. **Clarify action choices.** Apply observation lifetime rules consistently
   to equipment, projectiles and identification. Distinguish a proposal,
   server admission and observed resolution. Preserve disclosed failure reasons.
   Verify the free weapon socket with a two-grip bow, an admitted throw followed
   by failure, and an identification selection from a stale observation.
   A reader of the policy and trace should be able to explain the player's
   available evidence at selection and resolution, including any unknown cause.
6. **Improve full campaigns from evidence.** Select one consequential failure
   from fresh development runs. Save a reproducible decision, specify the
   intended choice and change the responsible layer. Repeat the focused case
   and development cohort with the same inputs. Early candidates include
   food budgeting for ascent and survival after acquisition. For a food change,
   measure return progress alongside extra foraging deaths and budget stops.
   Promote a policy helper to the API when it answers a reusable player question.

Each slice pairs the API change with the migrated policy, a short public example,
tests at the affected layers and an actual-process acceptance case. Start a bug
fix with a failing regression. Preserve existing callers while migrating the bot.
Follow the [testing policy](../development/testing.md) for verification.

## Independent acceptance criteria

**API quality.** Give a reviewer the public reference and the policy for obtaining
an item, retreating from a creature and descending a stair. For each, the reviewer
should be able to explain the observation, alternatives considered, chosen action
and evidence for updating memory. Needing to inspect the Rust adapter, join wire
records or reconstruct footprint geometry identifies work still to do. Replay
each example successfully. Give each shared rule one implementation owner and
a focused test.

**Campaign reliability.** The proposed first qualification target is at least
16 independently verified victories on 20 distinct, preselected held-out seeds.
Agree on the threshold before tuning. Run each seed once from a fresh save with
ordinary enemies and nutrition under the pinned configuration. Count all
scheduled seeds in the denominator. Report deaths, budget exhaustion, stalls
and infrastructure failures separately. Count a victory only when a fresh
ordinary player connection confirms victory with the Amulet. Record the first
bundled victory as an intermediate milestone. If held-out failures guide further
tuning, reserve fresh seeds for the next qualification.

## Server, protocol and common-client findings

Maintain the [findings log](../development/rogue-o-matic-findings.md) throughout.
For each surprising case, record the revision, reproduction, player-visible offer,
submitted action, admission, observed result, expected behavior and impact on the
author. Identify the suspected owner and distinguish observations from hypotheses.
Track any workaround and when it can be removed. Verify a fix with a focused
regression and rerun the affected bot decision.

For example, one door can have several disclosed approaches, which the client
can combine. A free weapon socket alone cannot establish whether the player has
enough grips for a bow. Investigate feasible-choice disclosure or a player-visible
rejection reason at the owning layer. Bring compatibility-changing proposals to
the maintainer with a reproduction and alternatives.
