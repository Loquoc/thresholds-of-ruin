---------------------------- MODULE ViewQueueAudit ----------------------------
EXTENDS Naturals, Sequences, TLC
CONSTANTS Views, Variant
ASSUME Views \in Nat \ {0}
VARIABLE s

View == [id : 1..Views, epoch : 0..1, generation : 0..1, ownStep : BOOLEAN]
RECURSIVE Flatten(_)
Flatten(batches) ==
    IF Len(batches) = 0 THEN <<>> ELSE Head(batches) \o Flatten(Tail(batches))

Prefix(seq, n) == SubSeq(seq, 1, n)
Last(seq) == seq[Len(seq)]

Init ==
    s = [source |-> <<>>, read |-> 0, gathered |-> <<>>, delivered |-> <<>>,
         marker |-> FALSE, ready |-> FALSE,
         captureUsed |-> FALSE, capturing |-> FALSE, crossedCapture |-> FALSE]

Produce ==
    /\ ~s.marker /\ Len(s.source) < Views
    /\ \E v \in View :
        /\ v.id = Len(s.source) + 1
        /\ IF Len(s.source) = 0 THEN v.epoch = 0 /\ v.generation = 0
           ELSE v.epoch >= Last(s.source).epoch /\ v.generation >= Last(s.source).generation
        /\ s' = [s EXCEPT !.source = Append(@, v)]

SendReady ==
    /\ ~s.marker /\ Len(s.source) > 0
    /\ s' = [s EXCEPT !.marker = TRUE]

CanConsume == ~s.capturing \/ Variant = "consume-during-capture"
RememberRead == s.crossedCapture \/ s.capturing
CanFold(old, new) ==
    /\ old.epoch = new.epoch \/ Variant = "merge-epochs"
    /\ old.generation = new.generation \/ Variant = "merge-generations"
    /\ ~new.ownStep \/ Variant = "merge-own-step"

Consume ==
    /\ CanConsume /\ s.read < Len(s.source)
    /\ LET v == s.source[s.read + 1] IN
           IF Len(s.gathered) = 0 \/ CanFold(Last(s.gathered), v)
           THEN s' = [s EXCEPT !.read = @ + 1, !.crossedCapture = RememberRead,
                               !.gathered = IF Variant = "drop-on-fold" /\ Len(@) > 0
                                           THEN <<v>> ELSE Append(@, v)]
           ELSE s' = [s EXCEPT !.read = @ + 1, !.crossedCapture = RememberRead,
                               !.delivered = Append(@, s.gathered), !.gathered = <<v>>]

Flush ==
    /\ CanConsume /\ Len(s.gathered) > 0
    /\ s' = [s EXCEPT !.delivered = Append(@, s.gathered), !.gathered = <<>>,
                       !.crossedCapture = RememberRead]

DeliverReady ==
    /\ CanConsume /\ s.marker /\ ~s.ready /\ s.read = Len(s.source)
    /\ Len(s.gathered) = 0 \/ Variant = "ready-on-read-ahead"
    /\ s' = [s EXCEPT !.ready = TRUE, !.crossedCapture = RememberRead]

Capture ==
    /\ ~s.captureUsed /\ s.read = 0 /\ ~s.ready
    /\ s' = [s EXCEPT !.captureUsed = TRUE, !.capturing = TRUE]

Release ==
    /\ s.capturing
    /\ s' = [s EXCEPT !.capturing = FALSE]

Next == Produce \/ SendReady \/ Consume \/ Flush \/ DeliverReady \/ Capture \/ Release
Spec == Init /\ [][Next]_s /\ WF_s(Next)

PreservesEveryViewInOrder ==
    Flatten(s.delivered) \o s.gathered = Prefix(s.source, s.read)
ReadyFollowsAppliedViews ==
    s.ready => Flatten(s.delivered) = s.source
SingleScope(batch) ==
    \A i \in 2..Len(batch) :
      /\ batch[i].epoch = batch[1].epoch
      /\ batch[i].generation = batch[1].generation
      /\ ~batch[i].ownStep
DoNotFoldAcrossBoundaries ==
    SingleScope(s.gathered) /\ \A i \in 1..Len(s.delivered) : SingleScope(s.delivered[i])
CaptureHoldsPresentation == ~s.crossedCapture
EventuallyReady == <> s.ready

FoldAbsent == ~(\E i \in 1..Len(s.delivered) : Len(s.delivered[i]) > 1)
ReadyHeldByLastViewAbsent == ~(s.marker /\ ~s.ready /\ s.read = Len(s.source) /\ Len(s.gathered) > 0)
CaptureBacklogAbsent == ~(s.capturing /\ Len(s.source) > s.read)
ResetBetweenViewsAbsent ==
    ~(s.ready /\ Len(s.source) >= 2 /\ Head(s.source).epoch # Last(s.source).epoch)
=============================================================================
