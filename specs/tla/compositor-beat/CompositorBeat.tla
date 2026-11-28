---------------------------- MODULE CompositorBeat ----------------------------
EXTENDS Naturals, TLC

CONSTANTS Steps, Lead, Variant
ASSUME Steps \in Nat \ {0} /\ Lead \in Nat
VARIABLE s

Ends == {"running", "finished", "interrupted", "replaced"}
Settled == s.ack /\ s.outcome /\ s.permissions /\ s.picture
SendAllowed == s.end = "running" /\ s.sent < Steps /\ Settled /\ s.lag <= Lead
InputAllowed == s.end # "running" /\ Settled /\ s.lag = 0

Init ==
    s = [sent |-> 0, ack |-> TRUE, outcome |-> TRUE, permissions |-> TRUE,
         picture |-> TRUE, lag |-> 0, end |-> "running",
         skipped |-> FALSE, replaced |-> FALSE, returned |-> FALSE,
         badSend |-> FALSE, badReturn |-> FALSE]

Send ==
    /\ s.end = "running" /\ s.sent < Steps /\ s.lag <= Lead
    /\ (Settled
        \/ (Variant = "receipt-only" /\ s.ack)
        \/ (Variant = "playback-only" /\ s.skipped /\ s.lag = 0))
    /\ s' = [s EXCEPT !.sent = @ + 1, !.ack = FALSE, !.outcome = FALSE,
                       !.permissions = FALSE, !.picture = FALSE, !.skipped = FALSE,
                       !.badSend = @ \/ ~SendAllowed]

ReceiveReceipt ==
    /\ ~s.ack
    /\ s' = [s EXCEPT !.ack = TRUE]

ReceiveOutcome ==
    /\ ~s.outcome
    /\ s' = [s EXCEPT !.outcome = TRUE, !.lag = @ + 2]

ReceivePermissions ==
    /\ s.outcome /\ ~s.permissions
    /\ s' = [s EXCEPT !.permissions = TRUE]

ReceivePicture ==
    /\ s.outcome /\ ~s.picture
    /\ s' = [s EXCEPT !.picture = TRUE]

Play ==
    /\ s.lag > 0
    /\ s' = [s EXCEPT !.lag = @ - 1]

Skip ==
    /\ s.lag > 0
    /\ s' = [s EXCEPT !.lag = 0, !.skipped = TRUE]

PlanEnds ==
    /\ s.end = "running" /\ s.sent = Steps /\ Settled
    /\ s' = [s EXCEPT !.end = "finished"]

Interrupt ==
    /\ s.end = "running" /\ s.sent > 0 /\ s.outcome
    /\ s' = [s EXCEPT !.end = "interrupted"]

Replace ==
    /\ ~s.returned /\ ~s.replaced
    \* Abstract the installed replacement and its usable view into one step.
    \* No old pending-message delivery is modeled after this scope boundary.
    /\ s' = [s EXCEPT !.replaced = TRUE,
                       !.end = IF Variant = "retain-old-plan" THEN @ ELSE "replaced",
                       !.lag = IF Variant = "retain-old-frames" THEN @ ELSE 0,
                       !.ack = TRUE, !.outcome = TRUE, !.permissions = TRUE, !.picture = TRUE]

ReturnControl ==
    /\ ~s.returned /\ s.end # "running" /\ Settled
    /\ (s.lag = 0 \/ Variant = "return-before-drain")
    /\ s' = [s EXCEPT !.returned = TRUE, !.badReturn = @ \/ ~InputAllowed]

Next ==
    Send \/ ReceiveReceipt \/ ReceiveOutcome \/ ReceivePermissions \/ ReceivePicture
    \/ Play \/ Skip \/ PlanEnds \/ Interrupt \/ Replace \/ ReturnControl

Spec == Init /\ [][Next]_s /\ WF_s(Next)

TypeOK ==
    s \in [sent : 0..Steps, ack : BOOLEAN, outcome : BOOLEAN, permissions : BOOLEAN,
           picture : BOOLEAN, lag : 0..(2 * Steps), end : Ends,
           skipped : BOOLEAN, replaced : BOOLEAN, returned : BOOLEAN,
           badSend : BOOLEAN, badReturn : BOOLEAN]

AuthorizedContinuation == ~s.badSend
ReplacementStopsPlan == s.replaced => s.end = "replaced"
ReplacementFlushes == s.replaced => s.lag = 0
InputAfterDrain == ~s.badReturn
EventuallyReturns == <> s.returned

ReceiptFirstAbsent == ~(s.sent = 1 /\ s.ack /\ ~s.outcome)
OutcomeFirstAbsent == ~(s.sent = 1 /\ s.outcome /\ ~s.ack)
SkipWithoutPermissionAbsent == ~(s.sent = 1 /\ s.skipped /\ ~s.permissions)
SecondStepWhilePlayingAbsent == ~(s.sent = 2 /\ ~s.ack /\ s.lag > 0)
OrdinaryReturnAbsent == ~(s.returned /\ s.end = "finished")
ReplacementReturnAbsent == ~(s.returned /\ s.end = "replaced" /\ s.sent > 0)
=============================================================================
