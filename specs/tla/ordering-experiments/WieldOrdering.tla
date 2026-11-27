------------------------------ MODULE WieldOrdering ------------------------------
EXTENDS TLC

CONSTANT Variant
VARIABLE s

Phases == {"waiting", "cleared", "occupied", "interrupted"}
Terminal == Phases \ {"waiting"}

Init ==
    s = [ phase |-> "waiting", ack |-> FALSE, update |-> FALSE,
          learned |-> "waiting", current |-> FALSE, picture |-> "unknown",
          available |-> FALSE, decision |-> "pending" ]

Resolve ==
    /\ s.phase = "waiting"
    /\ \E result \in Terminal :
           s' = [s EXCEPT !.phase = result]

DeliverUpdate ==
    /\ s.phase \in Terminal /\ ~s.update
    /\ s' = [s EXCEPT !.update = TRUE,
                       !.learned = IF s.ack THEN s.phase ELSE @]

DeliverReceipt ==
    /\ ~s.ack
    \* If the removal already ended in this commit, its update comes first.
    /\ (s.phase = "waiting" \/ s.update)
    /\ s' = [s EXCEPT !.ack = TRUE,
                       !.learned = IF Variant = "ignore-receipt-phase" THEN @ ELSE s.phase]

RefreshPicture ==
    /\ s.update /\ ~s.current
    /\ \E knownSlot \in (IF s.phase = "cleared" THEN {"empty", "occupied"} ELSE {"occupied"}),
          incoming \in BOOLEAN :
           s' = [s EXCEPT !.current = TRUE, !.picture = knownSlot, !.available = incoming]

Continue ==
    /\ s.decision = "pending"
    /\ s.learned \in Terminal
    /\ s.learned = "interrupted" \/ s.current
    /\ s' = [s EXCEPT !.decision =
           IF s.learned \in {"cleared", "occupied"}
              /\ (Variant = "ignore-slot" \/ s.picture = "empty")
              /\ (Variant = "ignore-incoming" \/ s.available)
           THEN "equip" ELSE "stop"]

Next == Resolve \/ DeliverUpdate \/ DeliverReceipt \/ RefreshPicture \/ Continue
Spec == Init /\ [][Next]_s /\ WF_s(Next)

TypeOK ==
    s \in [phase : Phases, ack : BOOLEAN, update : BOOLEAN, learned : Phases,
           current : BOOLEAN, picture : {"unknown", "empty", "occupied"},
           available : BOOLEAN, decision : {"pending", "stop", "equip"}]

TerminalNotLost ==
    (s.update /\ s.ack) => s.learned = s.phase

EquipUsesCurrentEmptySlot ==
    s.decision = "equip" =>
        s.current /\ s.picture = "empty" /\ s.learned = "cleared"

EquipUsesAvailableItem == s.decision = "equip" => s.available

EventuallyFinishes == <> (s.decision # "pending")

\* Both delivery orders and the refusal must be represented, not assumed.
ImmediateOrderAbsent == ~(s.update /\ ~s.ack /\ s.phase = "occupied")
DelayedOrderAbsent == ~(s.ack /\ ~s.update /\ s.phase = "waiting")
RefusalAbsent == ~(s.decision = "stop" /\ s.phase = "occupied")
ReoccupiedAbsent == ~(s.decision = "stop" /\ s.phase = "cleared" /\ s.picture = "occupied")
UnavailableAbsent == ~(s.decision = "stop" /\ s.phase = "cleared" /\ s.picture = "empty" /\ ~s.available)
EquipAbsent == ~(s.decision = "equip")
=============================================================================
