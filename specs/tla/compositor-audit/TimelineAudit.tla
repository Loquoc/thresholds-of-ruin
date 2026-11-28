----------------------------- MODULE TimelineAudit -----------------------------
EXTENDS Naturals, Sequences, FiniteSets, TLC
CONSTANTS Events, Variant
VARIABLE s

Actors == {"orc", "rat"}
Open == 3
MaxTick == Open - 1
MinOf(set) == CHOOSE x \in set : \A y \in set : x <= y
LastOf(set) == IF set = {} THEN 0 ELSE CHOOSE x \in set : \A y \in set : y <= x
Floors(xs, a) == {xs[i].start : i \in {j \in 1..Len(xs) : xs[j].actor = a}}
FrameCounter == s.frame + 1
CanRecord == s.frame < Events

Init ==
    s = [tick |-> 0, frame |-> 0, epoch |-> 0, spans |-> <<>>, flights |-> <<>>,
         hits |-> <<>>, dead |-> [a \in Actors |-> Open], resetAfterHit |-> FALSE]

Tick ==
    /\ s.tick < MaxTick
    /\ s' = [s EXCEPT !.tick = @ + 1]

Activity ==
    /\ CanRecord
    /\ \E actor \in Actors, start \in 0..s.tick :
        /\ s.dead[actor] = Open
        /\ \A earlier \in Floors(s.spans, actor) : earlier <= start
        /\ LET bounded == [i \in 1..Len(s.spans) |->
                   LET old == s.spans[i] IN
                   IF old.end = Open /\ old.start <= start
                       /\ (old.actor = actor \/ Variant = "cross-actor-close")
                   THEN [old EXCEPT !.end = start] ELSE old]
               incoming == [actor |-> actor, start |-> start, end |-> Open,
                            frame |-> FrameCounter, reported |-> s.tick, epoch |-> s.epoch]
           IN s' = [s EXCEPT !.frame = FrameCounter, !.spans = Append(bounded, incoming)]

Death ==
    /\ CanRecord
    /\ \E actor \in Actors :
        /\ s.dead[actor] = Open
        /\ s' = [s EXCEPT !.frame = FrameCounter, !.dead[actor] = s.tick,
                   !.spans = [i \in 1..Len(@) |->
                      LET old == @[i] IN
                      IF old.actor = actor /\ old.end = Open /\ Variant # "no-death-close"
                      THEN [old EXCEPT !.end = s.tick] ELSE old]]

Flight ==
    /\ CanRecord
    /\ \E actor \in Actors, effectTick \in 0..s.tick :
        s' = [s EXCEPT !.frame = FrameCounter,
               !.flights = Append(@, [actor |-> actor, tick |-> effectTick,
                           frame |-> FrameCounter, epoch |-> s.epoch])]

Related(actor) ==
    {i \in 1..Len(s.flights) :
        /\ s.flights[i].actor = actor \/ Variant = "ignore-caster"
        /\ s.flights[i].tick = s.tick \/ Variant = "ignore-tick"}

Hit ==
    /\ CanRecord
    /\ \E actor \in Actors :
        s' = [s EXCEPT !.frame = FrameCounter,
               !.hits = Append(@, [actor |-> actor, tick |-> s.tick,
                         frame |-> FrameCounter, epoch |-> s.epoch, link |-> LastOf(Related(actor))])]

Replace ==
    /\ s.epoch = 0 /\ s.frame > 0
    /\ s' = [s EXCEPT !.epoch = 1, !.spans = <<>>, !.flights = <<>>,
                !.resetAfterHit = Len(s.hits) > 0,
                !.hits = IF Variant = "retain-reactions" THEN @ ELSE <<>>,
                !.dead = [a \in Actors |-> Open]]

Next == Tick \/ Activity \/ Death \/ Flight \/ Hit \/ Replace
Spec == Init /\ [][Next]_s

ExpectedEnd(i) ==
    LET x == s.spans[i]
        later == {s.spans[j].start :
                    j \in {k \in (i + 1)..Len(s.spans) : s.spans[k].actor = x.actor}}
    IN MinOf(later \cup {s.dead[x.actor]})

ExactSpanBoundary ==
    \A i \in 1..Len(s.spans) : s.spans[i].end = ExpectedEnd(i)

RangeOfSpans == {s.spans[i] : i \in 1..Len(s.spans)}
RangeOfFlights == {s.flights[i] : i \in 1..Len(s.flights)}
RangeOfHits == {s.hits[i] : i \in 1..Len(s.hits)}

FrameAndScope ==
    /\ \A x \in RangeOfSpans :
           x.epoch = s.epoch /\ x.start <= x.reported /\ x.frame <= s.frame
    /\ \A x \in RangeOfFlights \cup RangeOfHits : x.epoch = s.epoch /\ x.frame <= s.frame

LinksMatchPriorFacts ==
    \A h \in RangeOfHits :
        LET preceding == {i \in 1..Len(s.flights) :
                            /\ s.flights[i].actor = h.actor
                            /\ s.flights[i].tick = h.tick
                            /\ s.flights[i].frame < h.frame}
        IN h.link = LastOf(preceding)

ParallelSpansAbsent ==
    ~(\E i, j \in 1..Len(s.spans) :
        s.spans[i].actor # s.spans[j].actor /\ s.spans[i].end = Open /\ s.spans[j].end = Open)
AmbiguousFlightsAbsent ==
    ~(\E h \in RangeOfHits :
         Cardinality({i \in 1..Len(s.flights) :
            s.flights[i].actor = h.actor /\ s.flights[i].tick = h.tick /\ s.flights[i].frame < h.frame}) > 1)
ResetAfterHitAbsent == ~s.resetAfterHit
=============================================================================
