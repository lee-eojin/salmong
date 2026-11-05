use crate::ids::{BusId, Dir, PassengerId, StationId, Tick};
use crate::scenario::Shape;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Spawn { passenger: PassengerId, want: Shape },
    EditNetwork,
    Move { bus: BusId },
    Alight { bus: BusId, passenger: PassengerId },
    EndAlight { bus: BusId },
    Board { bus: BusId, passenger: PassengerId },
    Depart { bus: BusId },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Moving,
    Alighting,
    Boarding,
    Parked,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BusState {
    pub at: StationId,
    pub dir: Dir,
    pub phase: Phase,
    pub retiring: bool,
    pub riders: Vec<PassengerId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceState {
    pub network: Vec<Vec<StationId>>,
    pub buses: Vec<BusState>,
    pub queues: Vec<Vec<PassengerId>>,
    pub delivered: Vec<PassengerId>,
    pub edits: u32,
    pub wants: Vec<Shape>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceStep {
    pub tick: Tick,
    pub action: Action,
    pub before: TraceState,
    pub after: TraceState,
}
