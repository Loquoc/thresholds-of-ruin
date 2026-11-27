------------------------------- MODULE SupplyClaims -------------------------------
EXTENDS Naturals, FiniteSets, TLC

CONSTANTS Units, Variant
VARIABLE s

Actors == {"a", "b"}
Phases == {"idle", "queued", "held", "paused", "cancelled", "invalidated", "paid"}
HeldPhases == {"held", "paused"}
Held == {s.chosen[a] : a \in {b \in Actors : s.phase[b] \in HeldPhases}}
Spent == UNION {s.paid[a] : a \in Actors}

Init ==
    s = [ phase |-> [a \in Actors |-> "idle"],
          chosen |-> [a \in Actors |-> "none"],
          paid |-> [a \in Actors |-> {}],
          ground |-> Units, taken |-> {}, lost |-> {} ]

Queue(a, u) ==
    /\ s.phase[a] = "idle" /\ u \in s.ground
    /\ s' = [s EXCEPT !.phase[a] = "queued", !.chosen[a] = u]

Start(a) ==
    /\ s.phase[a] = "queued"
    /\ LET u == s.chosen[a]
           free == u \in s.ground /\ (Variant = "allow-double-hold" \/ u \notin Held)
       IN s' = [s EXCEPT !.phase[a] = IF free THEN "held" ELSE "invalidated"]

Pause(a) ==
    /\ s.phase[a] = "held"
    /\ s' = [s EXCEPT !.phase[a] = "paused"]

Resume(a) ==
    /\ s.phase[a] = "paused"
    /\ s' = [s EXCEPT !.phase[a] = "held"]

Cancel(a) ==
    /\ s.phase[a] \in HeldPhases
    /\ s' = [s EXCEPT !.phase[a] = "cancelled",
                       !.ground = IF Variant = "consume-on-cancel" THEN @ \ {s.chosen[a]} ELSE @]

Take(u) ==
    /\ u \in s.ground \ Held
    /\ s' = [s EXCEPT !.ground = @ \ {u}, !.taken = @ \cup {u}]

Invalidate(u) ==
    /\ u \in s.ground
    /\ s' = [s EXCEPT !.ground = @ \ {u}, !.lost = @ \cup {u},
                       !.phase = [a \in Actors |->
                           IF s.chosen[a] = u /\ @[a] \in HeldPhases
                           THEN "invalidated" ELSE @[a]]]

Resolve(a) ==
    /\ s.phase[a] = "held" /\ s.chosen[a] \in s.ground
    /\ s' = [s EXCEPT !.phase[a] = "paid", !.paid[a] = @ \cup {s.chosen[a]},
                       !.ground = @ \ {s.chosen[a]}]

Retry(a) ==
    /\ s.phase[a] = "paid"
    /\ IF Variant = "pay-retry"
          THEN \E u \in s.ground \ Held :
                 s' = [s EXCEPT !.paid[a] = @ \cup {u}, !.ground = @ \ {u}]
          ELSE UNCHANGED s

Next ==
    \/ \E a \in Actors, u \in Units : Queue(a, u)
    \/ \E a \in Actors : Start(a) \/ Pause(a) \/ Resume(a) \/ Cancel(a) \/ Resolve(a) \/ Retry(a)
    \/ \E u \in Units : Take(u) \/ Invalidate(u)

Spec == Init /\ [][Next]_s

TypeOK ==
    s \in [phase : [Actors -> Phases], chosen : [Actors -> Units \cup {"none"}],
           paid : [Actors -> SUBSET Units], ground : SUBSET Units,
           taken : SUBSET Units, lost : SUBSET Units]

ExclusiveClaim ==
    \A a, b \in Actors :
        (a # b /\ s.phase[a] \in HeldPhases /\ s.phase[b] \in HeldPhases)
            => s.chosen[a] # s.chosen[b]

HoldBackedByStock == Held \subseteq s.ground

ConservesStock ==
    LET buckets == {s.ground, s.taken, s.lost, Spent}
    IN /\ UNION buckets = Units
       /\ Cardinality(s.ground) + Cardinality(s.taken)
          + Cardinality(s.lost) + Cardinality(Spent) = Cardinality(Units)

PaidAtMostOnce ==
    /\ \A a \in Actors : Cardinality(s.paid[a]) <= 1
    /\ s.paid["a"] \cap s.paid["b"] = {}

ReclaimAbsent ==
    ~(s.phase["a"] = "cancelled" /\ s.phase["b"] = "held" /\ s.chosen["a"] = s.chosen["b"])

PartialTakeAbsent ==
    ~(s.phase["a"] = "paused" /\ s.taken # {} /\ s.chosen["a"] \in s.ground)
=============================================================================
