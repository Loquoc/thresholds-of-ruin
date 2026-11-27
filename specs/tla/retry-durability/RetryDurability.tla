-------------------------- MODULE RetryDurability --------------------------
EXTENDS Naturals, Sequences, FiniteSets

CONSTANTS RequestIds, Items, MaxEpoch

ASSUME /\ IsFiniteSet(RequestIds) /\ RequestIds # {}
       /\ IsFiniteSet(Items) /\ Items # {}
       /\ MaxEpoch \in Nat /\ MaxEpoch >= 1

Envelope == [id : RequestIds, item : Items, basis : 1..MaxEpoch]
Results == {"none", "admitted", "receipt", "conflict", "stale"}
NoReply == [kind |-> "none", request |-> CHOOSE r \in Envelope : TRUE,
            before |-> <<>>]
Prefix(xs, n) == SubSeq(xs, 1, n)
IsPrefix(xs, ys) == Len(xs) <= Len(ys) /\ xs = Prefix(ys, Len(xs))
HasId(xs, id) == \E i \in DOMAIN xs : xs[i].id = id
(* In this one-user/actor/branch model the ordinary pickup's matching
   payload is the item. The attachment basis guards only fresh admission. *)
HasExact(xs, request) ==
    \E i \in DOMAIN xs :
      xs[i].id = request.id /\ xs[i].item = request.item
Reply(kind, request, before) ==
    [kind |-> kind, request |-> request, before |-> before]

VARIABLES attached, epoch, admitted, durable, save, promised, last
vars == <<attached, epoch, admitted, durable, save, promised, last>>

(* save.target is frozen at the save request, not at the reply. promised
   and last are evidence for checks, not inputs to the server's decisions. *)
NoSave == [active |-> FALSE, target |-> <<>>]

Init ==
    /\ attached = TRUE /\ epoch = 1
    /\ admitted = <<>> /\ durable = <<>> /\ promised = <<>>
    /\ save = NoSave /\ last = NoReply

Submit(request) ==
    /\ attached
    /\ LET known == HasId(admitted, request.id)
           kind == IF known
                   THEN IF HasExact(admitted, request) THEN "receipt"
                        ELSE "conflict"
                   ELSE IF request.basis # epoch THEN "stale"
                        ELSE "admitted"
       IN /\ last' = Reply(kind, request, admitted)
          /\ admitted' = IF kind = "admitted" THEN Append(admitted, request)
                         ELSE admitted
    /\ UNCHANGED <<attached, epoch, durable, save, promised>>

BeginSave ==
    /\ attached /\ ~save.active
    /\ save' = [active |-> TRUE, target |-> admitted]
    /\ last' = NoReply
    /\ UNCHANGED <<attached, epoch, admitted, durable, promised>>

(* Storage can commit any larger accepted prefix, including entries beyond
   a save's captured target. Receipts and admissions share that prefix. *)
Persist ==
    /\ \E n \in (Len(durable) + 1)..Len(admitted) :
          durable' = Prefix(admitted, n)
    /\ last' = NoReply
    /\ UNCHANGED <<attached, epoch, admitted, save, promised>>

CompleteSave ==
    /\ save.active
    /\ IsPrefix(save.target, durable)
    /\ promised' = save.target
    /\ save' = NoSave /\ last' = NoReply
    /\ UNCHANGED <<attached, epoch, admitted, durable>>

Detach ==
    /\ attached /\ attached' = FALSE
    /\ save' = NoSave /\ last' = NoReply
    /\ UNCHANGED <<epoch, admitted, durable, promised>>

Attach ==
    /\ ~attached /\ epoch < MaxEpoch
    /\ attached' = TRUE /\ epoch' = epoch + 1
    /\ last' = NoReply
    /\ UNCHANGED <<admitted, durable, save, promised>>

Crash ==
    /\ attached' = FALSE /\ admitted' = durable
    /\ save' = NoSave /\ last' = NoReply
    /\ UNCHANGED <<epoch, durable, promised>>

Next ==
    \/ \E request \in Envelope : Submit(request)
    \/ BeginSave \/ Persist \/ CompleteSave \/ Detach \/ Attach \/ Crash
Spec == Init /\ [][Next]_vars

TypeOK ==
    /\ attached \in BOOLEAN /\ epoch \in 1..MaxEpoch
    /\ admitted \in Seq(Envelope) /\ durable \in Seq(Envelope)
    /\ save \in [active : BOOLEAN, target : Seq(Envelope)]
    /\ promised \in Seq(Envelope)
    /\ last \in [kind : Results, request : Envelope,
                 before : Seq(Envelope)]

DurableIsAdmittedPrefix == IsPrefix(durable, admitted)
SavePromiseSurvives == IsPrefix(promised, durable)
OneAdmissionPerId ==
    Cardinality({admitted[i].id : i \in DOMAIN admitted}) = Len(admitted)

ReplyMatchesAdmission ==
    IF last.kind = "none" THEN TRUE ELSE
        LET request == last.request
            known == HasId(last.before, request.id)
            exact == HasExact(last.before, request)
        IN CASE last.kind = "admitted" ->
                 /\ ~known /\ request.basis = epoch
                 /\ admitted = Append(last.before, request)
             [] last.kind = "receipt" -> exact /\ admitted = last.before
             [] last.kind = "conflict" ->
                 known /\ ~exact /\ admitted = last.before
             [] last.kind = "stale" ->
                 ~known /\ request.basis # epoch /\ admitted = last.before
=============================================================================
