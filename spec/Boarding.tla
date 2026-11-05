------------------------------ MODULE Boarding ------------------------------
EXTENDS Naturals, Sequences, FiniteSets
CONSTANTS Stations, Routes, Buses, Capacity, MaxPassengers, MaxEdits, Fault, Networks
Range(s) == {s[i] : i \in DOMAIN s}
Min(S) == CHOOSE v \in S : \A x \in S : v <= x
RemoveAt(s, i) == SubSeq(s, 1, i - 1) \o SubSeq(s, i + 1, Len(s))
Hop == 10
Transfer == 30
Inf == 1000000
NoStop == 0
NoShape == "Unset"
Plus(a, b) == IF a + b >= Inf THEN Inf ELSE a + b
Passengers == 1..MaxPassengers
Shape == <<"Circle", "Triangle", "Circle", "Square">>
BusRoute == <<1, 2>>
Shapes == Range(Shape)
Dirs == {"Fwd", "Back"}
Phases == {"Moving", "Alighting", "Boarding", "Parked"}
N0 == <<<<1, 2, 3>>, <<3, 4>>>>
N1 == <<<<1, 2, 3>>, <<2, 4>>>>
N2 == <<<<1, 2>>, <<3, 4>>>>
N3 == <<<<3, 2, 1>>, <<3, 4>>>>
N4 == <<<<1, 2, 3>>, <<>>>>
HandNetworks == {N0, N1, N2, N3, N4}
ASSUME /\ DOMAIN Shape = Stations
       /\ DOMAIN BusRoute = Buses
       /\ Range(BusRoute) \subseteq Routes
       /\ N0 \in Networks
       /\ \A n \in Networks :
            /\ DOMAIN n = Routes
            /\ \A r \in Routes :
                 /\ n[r] \in Seq(Stations)
                 /\ Len(n[r]) = 0 \/ Len(n[r]) >= 2
                 /\ Cardinality(Range(n[r])) = Len(n[r])
            /\ \E b \in Buses : Len(n[BusRoute[b]]) > 0
StopKeys(n) == UNION { {<<r, d, st>> : d \in Dirs, st \in Range(n[r])} : r \in Routes }
NextStop(n, k) ==
    LET s == n[k[1]]
        i == CHOOSE j \in DOMAIN s : s[j] = k[3]
        j == IF k[2] = "Fwd" THEN i + 1 ELSE i - 1
    IN IF j \in DOMAIN s THEN s[j] ELSE NoStop
Boardable(n, st) == {k \in StopKeys(n) : k[3] = st /\ NextStop(n, k) # NoStop}
Relax(n, w, x) ==
    [cost |-> [st \in Stations |-> IF Shape[st] = w THEN 0 ELSE
        Min({Inf} \cup {Plus(Transfer, x.ride[k]) : k \in Boardable(n, st)})],
     ride |-> [k \in StopKeys(n) |-> IF Shape[k[3]] = w THEN 0 ELSE
        IF NextStop(n, k) = NoStop THEN x.cost[k[3]] ELSE
        Min({x.cost[k[3]], Plus(Hop, x.ride[<<k[1], k[2], NextStop(n, k)>>])})]]
RECURSIVE Settle(_, _, _)
Settle(n, w, x) == LET y == Relax(n, w, x) IN IF y = x THEN x ELSE Settle(n, w, y)
Solve(n, w) ==
    LET x == Settle(n, w, [cost |-> [st \in Stations |-> Inf], ride |-> [k \in StopKeys(n) |-> Inf]])
    IN [cost |-> x.cost, ride |-> x.ride,
        board |-> [st \in Stations |-> {<<k[1], k[2]>> : k \in
            {j \in Boardable(n, st) : x.cost[st] < Inf /\ Plus(Transfer, x.ride[j]) = x.cost[st]}}],
        stay |-> [k \in StopKeys(n) |-> IF NextStop(n, k) = NoStop THEN FALSE ELSE
            Plus(Hop, x.ride[<<k[1], k[2], NextStop(n, k)>>]) <= x.cost[k[3]]]]
Table == [n \in Networks |-> [w \in Shapes |-> Solve(n, w)]]
TableGolden ==
    LET a == Solve(N0, "Square")
        b == Solve(N1, "Square")
        c == Solve(N2, "Square")
        d == Solve(N0, "Circle")
        e == Solve(N2, "Triangle")
        f == Solve(N4, "Square")
    IN /\ a.cost = <<90, 80, 40, 0>>
       /\ a.ride[<<1, "Fwd", 2>>] = 50
       /\ a.stay[<<1, "Fwd", 2>>]
       /\ a.board[2] = {<<1, "Fwd">>}
       /\ b.cost = <<80, 40, 80, 0>>
       /\ ~b.stay[<<1, "Fwd", 2>>]
       /\ b.board[2] = {<<2, "Fwd">>}
       /\ c.cost = <<Inf, Inf, 40, 0>>
       /\ c.stay[<<1, "Fwd", 1>>]
       /\ c.board[1] = {}
       /\ d.cost = <<0, 40, 0, 40>>
       /\ d.board[2] = {<<1, "Fwd">>, <<1, "Back">>}
       /\ e.cost = <<40, 0, Inf, Inf>>
       /\ f.cost = <<Inf, Inf, Inf, 0>>
ASSUME TableGolden
VARIABLES network, dir, retiring, queue, onboard, busAt, busPhase, delivered, edits, spawned, want
vars == <<network, dir, retiring, queue, onboard, busAt, busPhase, delivered, edits, spawned, want>>
OnRoute(b) == busAt[b] \in Range(network[BusRoute[b]])
MustLeave(b, p) ==
    IF Shape[busAt[b]] = want[p] THEN TRUE
    ELSE IF b \in retiring THEN TRUE
    ELSE IF ~OnRoute(b) THEN TRUE
    ELSE ~Table[network][want[p]].stay[<<BusRoute[b], dir[b], busAt[b]>>]
SeatFree(b) == IF Fault = "BoardAtCapacity" THEN Len(onboard[b]) <= Capacity ELSE Len(onboard[b]) < Capacity
BoardingOpen(b) == busPhase[b] = "Boarding" \/ (Fault = "BoardBeforeAlight" /\ busPhase[b] = "Alighting")
QueueAfterTransfer(st, p) == IF Fault = "SplitTransfer" THEN queue[st] ELSE Append(queue[st], p)
Delivers(st, p) == IF Fault = "DeliverAnywhere" THEN TRUE ELSE Shape[st] = want[p]
Restarts(b, n) ==
    /\ busPhase[b] \in {"Alighting", "Boarding"}
    /\ b \notin retiring
    /\ IF Fault = "RestartEditedOnly" THEN network[BusRoute[b]] # n[BusRoute[b]] ELSE TRUE
Serves(b, p) ==
    IF Fault = "BoardAnyBus" THEN TRUE
    ELSE <<BusRoute[b], dir[b]>> \in Table[network][want[p]].board[busAt[b]]
Eligible(b) ==
    LET eligible == {i \in DOMAIN queue[busAt[b]] : Serves(b, queue[busAt[b]][i])}
    IN IF Fault = "HeadOfLine" THEN eligible \cap {1} ELSE eligible
RelocationStops(b) ==
    IF Fault = "StuckOffRoute" THEN {} ELSE Range(network[BusRoute[b]])
CanBoard(b) == BoardingOpen(b) /\ SeatFree(b) /\ Eligible(b) # {}
Init ==
    /\ network = N0
    /\ dir = [b \in Buses |-> "Fwd"]
    /\ retiring = {}
    /\ queue = [st \in Stations |-> <<>>]
    /\ onboard = [b \in Buses |-> <<>>]
    /\ busAt = [b \in Buses |-> N0[BusRoute[b]][1]]
    /\ busPhase = [b \in Buses |-> "Boarding"]
    /\ delivered = {}
    /\ edits = 0
    /\ spawned = 0
    /\ want = [p \in Passengers |-> NoShape]
Spawn ==
    /\ spawned < MaxPassengers
    /\ \E st \in Stations : \E w \in Shapes \ {Shape[st]} :
         /\ want' = [want EXCEPT ![spawned + 1] = w]
         /\ queue' = [queue EXCEPT ![st] = Append(@, spawned + 1)]
    /\ spawned' = spawned + 1
    /\ UNCHANGED <<network, dir, retiring, onboard, busAt, busPhase, delivered, edits>>
Move(b) ==
    /\ busPhase[b] = "Moving"
    /\ busPhase' = [busPhase EXCEPT ![b] = "Alighting"]
    /\ UNCHANGED <<network, dir, retiring, queue, onboard, busAt, delivered, edits, spawned, want>>
Leaving(b) == {i \in DOMAIN onboard[b] : MustLeave(b, onboard[b][i])}
Alight(b) ==
    /\ busPhase[b] = "Alighting"
    /\ Leaving(b) # {}
    /\ LET i == Min(Leaving(b))
           p == onboard[b][i]
           st == busAt[b]
       IN /\ onboard' = [onboard EXCEPT ![b] = RemoveAt(@, i)]
          /\ IF Delivers(st, p)
              THEN /\ delivered' = delivered \cup {p}
                   /\ UNCHANGED queue
              ELSE /\ queue' = [queue EXCEPT ![st] = QueueAfterTransfer(st, p)]
                   /\ UNCHANGED delivered
    /\ UNCHANGED <<network, dir, retiring, busAt, busPhase, edits, spawned, want>>
EndAlight(b) ==
    /\ busPhase[b] = "Alighting"
    /\ Leaving(b) = {}
    /\ IF b \in retiring
        THEN /\ busPhase' = [busPhase EXCEPT ![b] = "Parked"]
             /\ UNCHANGED <<busAt, dir>>
        ELSE IF ~OnRoute(b)
        THEN \E st \in RelocationStops(b) :
             /\ busAt' = [busAt EXCEPT ![b] = st]
             /\ dir' = [dir EXCEPT ![b] = IF st = network[BusRoute[b]][Len(network[BusRoute[b]])] THEN "Back" ELSE "Fwd"]
             /\ busPhase' = [busPhase EXCEPT ![b] = "Moving"]
        ELSE /\ dir' = [dir EXCEPT ![b] = IF NextStop(network, <<BusRoute[b], dir[b], busAt[b]>>) = NoStop
                                             THEN IF @ = "Fwd" THEN "Back" ELSE "Fwd" ELSE @]
             /\ busPhase' = [busPhase EXCEPT ![b] = "Boarding"]
             /\ UNCHANGED busAt
    /\ UNCHANGED <<network, retiring, queue, onboard, delivered, edits, spawned, want>>
Board(b) ==
    /\ CanBoard(b)
    /\ LET i == Min(Eligible(b))
       IN /\ onboard' = [onboard EXCEPT ![b] = Append(@, queue[busAt[b]][i])]
          /\ queue' = [queue EXCEPT ![busAt[b]] = RemoveAt(@, i)]
    /\ UNCHANGED <<network, dir, retiring, busAt, busPhase, delivered, edits, spawned, want>>
Depart(b) ==
    /\ busPhase[b] = "Boarding"
    /\ ~CanBoard(b)
    /\ busAt' = [busAt EXCEPT ![b] = NextStop(network, <<BusRoute[b], dir[b], busAt[b]>>) ]
    /\ busPhase' = [busPhase EXCEPT ![b] = "Moving"]
    /\ UNCHANGED <<network, dir, retiring, queue, onboard, delivered, edits, spawned, want>>
EditNetwork ==
    /\ edits < MaxEdits
    /\ \E n \in Networks \ {network} :
         /\ network' = n
         /\ retiring' = retiring \cup {b \in Buses : Len(n[BusRoute[b]]) = 0}
         /\ busPhase' = [b \in Buses |-> IF Restarts(b, n) THEN "Alighting" ELSE busPhase[b]]
    /\ edits' = edits + 1
    /\ UNCHANGED <<dir, queue, onboard, busAt, delivered, spawned, want>>
BusStep(b) == Move(b) \/ Alight(b) \/ EndAlight(b) \/ Board(b) \/ Depart(b)
Next == Spawn \/ EditNetwork \/ (\E b \in Buses : BusStep(b))
Spec == Init /\ [][Next]_vars
LiveSpec == Spec /\ \A b \in Buses : WF_vars(BusStep(b))
TypeOK ==
    /\ network \in Networks
    /\ dir \in [Buses -> Dirs]
    /\ retiring \subseteq Buses
    /\ queue \in [Stations -> Seq(Passengers)]
    /\ onboard \in [Buses -> Seq(Passengers)]
    /\ busAt \in [Buses -> Stations]
    /\ busPhase \in [Buses -> Phases]
    /\ delivered \subseteq Passengers
    /\ edits \in 0..MaxEdits
    /\ spawned \in 0..MaxPassengers
    /\ want \in [Passengers -> Shapes \cup {NoShape}]
PlacesOf(p) ==
    UNION {{<<"queue", st, i>> : i \in {j \in DOMAIN queue[st] : queue[st][j] = p}} : st \in Stations}
    \cup UNION {{<<"onboard", b, i>> : i \in {j \in DOMAIN onboard[b] : onboard[b][j] = p}} : b \in Buses}
    \cup IF p \in delivered THEN {<<"delivered">>} ELSE {}
CapacitySafe == \A b \in Buses : Len(onboard[b]) <= Capacity
Conserved == \A p \in Passengers : Cardinality(PlacesOf(p)) = IF p <= spawned THEN 1 ELSE 0
NoWrongDeliveryStep == \A p \in delivered' \ delivered : \E b \in Buses :
    p \in Range(onboard[b]) /\ busPhase[b] # "Moving" /\ Shape[busAt[b]] = want[p]
NoWrongDelivery == [][NoWrongDeliveryStep]_vars
AlightBeforeBoardStep == \A b \in Buses : Len(onboard'[b]) > Len(onboard[b]) =>
    \A i \in DOMAIN onboard[b] : ~MustLeave(b, onboard[b][i])
AlightBeforeBoard == [][AlightBeforeBoardStep]_vars
TransferSoundStep == \A b \in Buses : \A p \in Range(onboard[b]) \ Range(onboard'[b]) :
    /\ busPhase[b] # "Moving"
    /\ p \in delivered' \/ queue'[busAt[b]] = Append(queue[busAt[b]], p)
TransferSound == [][TransferSoundStep]_vars
Potential(p) ==
    LET t == Table[network][want[p]]
        waiting == {st \in Stations : p \in Range(queue[st])}
        riding == {b \in Buses : p \in Range(onboard[b])}
    IN IF p \in delivered THEN 0
       ELSE IF waiting # {} THEN t.cost[CHOOSE st \in waiting : TRUE]
       ELSE LET b == CHOOSE x \in riding : TRUE
            IN IF b \in retiring \/ ~OnRoute(b) THEN t.cost[busAt[b]]
               ELSE t.ride[<<BusRoute[b], dir[b], busAt[b]>>]
Owed(p) == p <= spawned /\ p \notin delivered /\ Potential(p) < Inf
Boards(p) == \E b \in Buses : p \in Range(onboard'[b]) \ Range(onboard[b])
Hops(p) == \E b \in Buses : p \in Range(onboard[b]) /\ busAt'[b] # busAt[b]
Drop(p) == IF Boards(p) THEN Transfer ELSE IF Hops(p) THEN Hop ELSE 0
Less(a, d) == IF a = Inf THEN Inf ELSE a - d
NoPingPongStep ==
    network' = network => \A p \in 1..spawned : Potential(p)' = Less(Potential(p), Drop(p))
NoPingPong == [][NoPingPongStep]_vars
Delivery == \A p \in Passengers : []<>~Owed(p)
BusProgress == \A b \in Buses : []<>(busPhase[b] \in {"Moving", "Parked"})
=============================================================================
