----------------------------- MODULE SettleAudit -----------------------------
EXTENDS Naturals, TLC
CONSTANT Variant
VARIABLE s

Phases == {"queued", "started", "resolved", "failed"}
Results == {"pending", "done", "failed", "yielded", "lost", "unknown"}
Terminal == {"resolved", "failed"}
Owned == {"queued", "started"}

Init ==
    s = [phase |-> "queued", receipt |-> FALSE, permit |-> FALSE,
         completionPermit |-> FALSE, oldPermitSeen |-> FALSE, foreignSeen |-> FALSE,
         failureKnown |-> FALSE, control |-> TRUE, reset |-> FALSE, result |-> "pending"]

Active == s.result = "pending"
Receipt ==
    /\ Active /\ ~s.receipt
    \* Fresh gameplay publishes its queued receipt before execution; the
    \* wider sensitivity model tests a receipt naming already finished work.
    /\ (s.phase = "queued" \/ Variant = "terminal-receipt")
    /\ s' = [s EXCEPT !.receipt = TRUE]

Start ==
    /\ Active /\ s.phase = "queued"
    /\ (s.receipt \/ Variant = "terminal-receipt")
    /\ s' = [s EXCEPT !.phase = "started", !.permit = FALSE]

Finish ==
    /\ Active /\ s.phase \in Owned
    /\ (s.receipt \/ Variant = "terminal-receipt")
    /\ \E final \in Terminal :
           s' = [s EXCEPT !.phase = final,
                           !.permit = IF Variant = "reuse-started-permission" THEN @ ELSE FALSE,
                           !.completionPermit = FALSE,
                           !.failureKnown = s.receipt /\ final = "failed"]

Permission ==
    /\ Active /\ s.phase # "queued" /\ ~s.permit
    /\ s' = [s EXCEPT !.permit = TRUE, !.completionPermit = s.phase \in Terminal]

EarlierPermit ==
    /\ Active /\ ~s.oldPermitSeen
    /\ s' = [s EXCEPT !.oldPermitSeen = TRUE,
                       !.permit = @ \/ Variant = "accept-old-permission"]

ForeignEnd ==
    /\ Active /\ ~s.foreignSeen
    /\ s' = [s EXCEPT !.foreignSeen = TRUE,
                       !.failureKnown = @ \/ Variant = "accept-other-failure"]

Settle ==
    /\ Active /\ s.receipt /\ s.phase \in Terminal /\ s.permit
    /\ s' = [s EXCEPT !.result = IF s.failureKnown THEN "failed" ELSE "done"]

Yield ==
    /\ Active /\ s.receipt /\ (s.phase = "queued" \/ Variant = "yield-after-known-failure")
    /\ s' = [s EXCEPT !.result = "yielded"]

Lose ==
    /\ Active
    /\ s' = [s EXCEPT !.control = FALSE, !.result = "lost"]

Replace ==
    /\ Active
    /\ s' = [s EXCEPT !.reset = TRUE, !.result = IF Variant = "snapshot-is-done" THEN "done" ELSE "lost"]

Deadline ==
    /\ Active
    /\ s' = [s EXCEPT !.result = "unknown"]

Next == Receipt \/ Start \/ Finish \/ Permission \/ EarlierPermit \/ ForeignEnd
        \/ Settle \/ Yield \/ Lose \/ Replace \/ Deadline
Spec == Init /\ [][Next]_s /\ WF_s(Next)

TypeOK ==
    s \in [phase : Phases, receipt : BOOLEAN, permit : BOOLEAN,
           completionPermit : BOOLEAN, oldPermitSeen : BOOLEAN, foreignSeen : BOOLEAN,
           failureKnown : BOOLEAN, control : BOOLEAN, reset : BOOLEAN, result : Results]
SnapshotIsNotSuccess == s.reset => s.result = "lost"
DoneHasFreshBoundary ==
    s.result \in {"done", "failed"} => s.receipt /\ s.phase \in Terminal /\ s.completionPermit
CorrectResult ==
    /\ s.result = "done" => s.phase = "resolved"
    /\ s.result = "failed" => s.phase = "failed"
DoNotYieldFailure == s.result = "yielded" => s.phase = "queued"
EventuallyLeaves == <> (s.result # "pending")

StartedPermissionAbsent == ~(s.receipt /\ s.phase = "started" /\ s.permit /\ Active)
QueuedYieldAbsent == ~(s.result = "yielded")
TimedOutAfterReceiptAbsent == ~(s.result = "unknown" /\ s.receipt)
RetainedFailureAbsent == ~(s.result = "failed" /\ s.foreignSeen)
=============================================================================
