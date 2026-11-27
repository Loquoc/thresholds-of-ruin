# Two bounded ordering experiments

These are local experiments, not server proofs or CI checks. Run both models,
their reachability witnesses, and deliberate failure controls with Python 3,
Java, and the checksum-verified
[TLC 2.19 jar from TLA+ tools v1.7.4](https://github.com/tlaplus/tlaplus/releases/tag/v1.7.4):

```sh
python3 specs/tla/ordering-experiments/check.py /path/to/tla2tools.jar
```

## ASCII Wield handoff

**Question:** When a removal's status update arrives before its receipt identifies
the intention, can the shortcut lose the terminal phase or issue an equip from
an out-of-date assumption?

One shortcut and one removal are modelled. The same-commit update comes before
the receipt; a slow removal can acknowledge before its terminal update. A
successful removal does not itself prove that the latest socket is still empty
or that the intended replacement is still available. The model assumes the
ordinary ordered stream, that the picture and readiness eventually become
current, and that the client continues processing. The weak fairness check is
conditional on that finite progression, not on player input, network progress
or a paused action resuming itself. This model stops at submitting an equip or
ending the shortcut; it does not prove eventual server execution of equip.

TLC checked 39 states and found no violations with these assumptions. It
recovered the known pre-fix missing-phase failure when receipt handling was
deliberately disabled. Disabling either fresh slot or availability checks
let the model equip when it should stop. Positive witnesses exercised both
delivery orders, refusal, an eligible replacement, and both revalidation stops.

Implementation anchors: `note_admission`, `observe_weapon_swap`, and
`continue_weapon_swap` in `crates/client-ascii/src/model.rs` on PR #89.
The receipt-phase and native swap tests already cover the known failure.
A useful focused follow-up is to verify the shortcut stops when the fresh
post-removal picture says the intended replacement is no longer usable. This
is a suggested coverage check, not an observed client defect.

## Finite supply claims

**Question:** While two actors contend for a finite ground stack, can queuing,
pausing, taking, cancelling, invalidating or resolving spend or reserve the
same stock incorrectly?

Two actors have at most one intention each, with one, two or three units.
Units are abstract tokens counting physical stock, not in-game public IDs:
the real game binds exact stack IDs and quantities. Ground stock remains
authoritative while held. Ordinary pickup uses only free units; cancellation
releases without spending; forced invalidation removes the affected unit from
this ground pool, not necessarily from the world. Resolution spends once
regardless of effect, and duplicate acknowledgement does not pay again.
The model does not cover Lua, multi-unit or multi-stack costs, reciprocal
claim serialization, cooldowns or actual storage recovery.

TLC checked 67, 635 and 3,933 distinct states for one, two and three units,
respectively, without invariant violations. The controls independently caught
overlapping claims, physical stock lost on cancellation, and a paid retry
charging a different unit. Reachability checks confirmed another actor can
claim the same unit after cancellation, and that taking a free unit can
interleave with a paused hold on the other unit.

Implementation anchors: [paid item work and finite supply claims](../../../docs/reference/equipment.md#paid-item-work-and-finite-supply-claims)
and `crates/server/tests/it/supply_lifecycle.rs`. The existing contention,
cancellation, and save/retry cases passed locally. The added
`paused_claim_allows_only_the_free_unit_to_be_taken_and_then_spends_once`
case checks the combined behavior: reject an attempt to take both units,
transfer only the free unit on a separate request, then resume and spend the
held unit exactly once. The implementation passed without a production change.
