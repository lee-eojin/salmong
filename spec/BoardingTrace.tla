---------------------------- MODULE BoardingTrace ----------------------------
EXTENDS Naturals, Sequences, FiniteSets, BoardingTraceData
VARIABLES network, dir, retiring, queue, onboard, busAt, busPhase, delivered, edits, spawned, want, i
vars == <<network, dir, retiring, queue, onboard, busAt, busPhase, delivered, edits, spawned, want>>
Range(s) == {s[k] : k \in DOMAIN s}
TraceNetworks == {Trace[k].before.network : k \in DOMAIN Trace}
    \cup {Trace[k].after.network : k \in DOMAIN Trace}
    \cup {<<<<1, 2, 3>>, <<3, 4>>>>}
B == INSTANCE Boarding WITH Stations <- 1..4, Routes <- 1..2, Buses <- 1..2,
    Capacity <- TraceCapacity, MaxPassengers <- TraceMaxPassengers, MaxEdits <- TraceMaxEdits,
    Fault <- "None", Networks <- TraceNetworks
Is(s) ==
    /\ network = s.network /\ dir = s.dir /\ retiring = s.retiring /\ queue = s.queue
    /\ onboard = s.onboard /\ busAt = s.busAt /\ busPhase = s.busPhase /\ delivered = s.delivered
    /\ edits = s.edits /\ spawned = s.spawned /\ want = s.want
IsNext(s) ==
    /\ network' = s.network /\ dir' = s.dir /\ retiring' = s.retiring /\ queue' = s.queue
    /\ onboard' = s.onboard /\ busAt' = s.busAt /\ busPhase' = s.busPhase /\ delivered' = s.delivered
    /\ edits' = s.edits /\ spawned' = s.spawned /\ want' = s.want
Moved(e) ==
    CASE e.act = "Spawn" -> B!Spawn /\ e.bus = 0 /\ e.p = spawned'
      [] e.act = "EditNetwork" -> B!EditNetwork /\ e.bus = 0 /\ e.p = 0
      [] e.act = "Move" -> B!Move(e.bus) /\ e.p = 0
      [] e.act = "Alight" -> B!Alight(e.bus) /\ e.p \in Range(onboard[e.bus]) \ Range(onboard'[e.bus])
      [] e.act = "EndAlight" -> B!EndAlight(e.bus) /\ e.p = 0
      [] e.act = "Board" -> B!Board(e.bus) /\ e.p \in Range(onboard'[e.bus]) \ Range(onboard[e.bus])
      [] e.act = "Depart" -> B!Depart(e.bus) /\ e.p = 0
      [] OTHER -> FALSE
Init == i = 1 /\ Is(Trace[1].before) /\ B!TypeOK /\ B!CapacitySafe /\ B!Conserved
Step ==
    /\ i <= Len(Trace)
    /\ Is(Trace[i].before)
    /\ Moved(Trace[i])
    /\ IsNext(Trace[i].after)
    /\ i' = i + 1
Done == i = Len(Trace) + 1 /\ UNCHANGED <<vars, i>>
Spec == Init /\ [][Step \/ Done]_<<vars, i>>
TypeOK == B!TypeOK
CapacitySafe == B!CapacitySafe
Conserved == B!Conserved
=============================================================================
