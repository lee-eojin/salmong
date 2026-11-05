use crate::balance::Balance;
use std::collections::VecDeque;

use crate::ids::{BusId, Dir, PassengerId, RouteId, StationId, Tick};
use crate::network::{MAX_ROUTES, NetworkSnapshot};
use crate::routing::{DijkstraRouter, RouteTable, Router, board_bit};
use crate::scenario::{Scenario, Shape};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusMode {
    Active { route: RouteId, dir: Dir },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusPhase {
    Moving {
        from: StationId,
        to: StationId,
        progress: u32,
    },
    Alighting,
    Boarding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    AddRoute { stops: Vec<StationId> },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimEvent {
    CommandRejected,
    Boarded {
        bus: BusId,
        passenger: PassengerId,
        station: StationId,
    },
    Alighted {
        bus: BusId,
        passenger: PassengerId,
        station: StationId,
    },
    Delivered {
        bus: BusId,
        passenger: PassengerId,
        station: StationId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Passenger {
    pub id: PassengerId,
    pub want: Shape,
}

#[derive(Clone, Debug)]
struct Bus {
    id: BusId,
    mode: BusMode,
    phase: BusPhase,
    at: StationId,
    last_arrival: Option<(StationId, Tick)>,
    timer: u32,
    riders: Vec<Passenger>,
}

pub struct Sim {
    balance: Balance,
    shapes: Vec<Shape>,
    routes: Vec<Option<Vec<StationId>>>,
    rev: u64,
    table: RouteTable,
    dirty: bool,
    free_buses: u16,
    next_bus_id: u16,
    buses: Vec<Bus>,
    cmds: Vec<Command>,
    tick: Tick,
    queues: Vec<[VecDeque<(u64, Passenger)>; 4]>,
    next_seq: u64,
    next_passenger: u32,
    score: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BusView {
    pub id: BusId,
    pub route: RouteId,
    pub dir: Dir,
    pub phase: BusPhase,
    pub at: StationId,
    pub last_arrival: Option<(StationId, Tick)>,
    pub riders: Vec<PassengerId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimView {
    pub tick: Tick,
    pub free_buses: u16,
    pub route_count: usize,
    pub table_rev: u64,
    pub score: u64,
    pub queued: usize,
    pub buses: Vec<BusView>,
}

impl Sim {
    pub fn new(scenario: Scenario, _seed: u64, balance: Balance) -> Self {
        let shapes = scenario
            .stations
            .iter()
            .filter(|s| s.at_tick == 0)
            .map(|s| s.shape)
            .collect::<Vec<_>>();
        let net = NetworkSnapshot::new(0, shapes.clone(), vec![]).expect("start stations");
        let table = DijkstraRouter.solve(&net);
        let queues = shapes.iter().map(|_| Default::default()).collect();
        Self {
            balance,
            shapes,
            routes: Vec::new(),
            rev: 0,
            table,
            dirty: false,
            free_buses: balance.start_buses,
            next_bus_id: 0,
            buses: Vec::new(),
            cmds: Vec::new(),
            tick: Tick(0),
            queues,
            next_seq: 0,
            next_passenger: 0,
            score: 0,
        }
    }

    pub fn place_passenger(&mut self, station: StationId, want: Shape) -> PassengerId {
        let id = PassengerId(self.next_passenger);
        self.next_passenger += 1;
        self.enqueue(station, Passenger { id, want });
        id
    }

    pub fn queue(&mut self, cmd: Command) {
        self.cmds.push(cmd);
    }

    pub fn step(&mut self, events: &mut Vec<SimEvent>) {
        let tick = self.tick;
        self.run_commands(events);
        if self.dirty {
            self.recompute_table();
        }
        self.move_buses(tick);
        self.alight_buses(events);
        self.board_buses(events);
        self.tick = Tick(tick.0 + 1);
    }

    pub fn view(&self) -> SimView {
        SimView {
            tick: self.tick,
            free_buses: self.free_buses,
            route_count: self.routes.iter().filter(|r| r.is_some()).count(),
            table_rev: self.table.rev(),
            score: self.score,
            queued: self.queues.iter().flatten().map(VecDeque::len).sum(),
            buses: self
                .buses
                .iter()
                .map(|b| {
                    let BusMode::Active { route, dir } = b.mode;
                    BusView {
                        id: b.id,
                        route,
                        dir,
                        phase: b.phase.clone(),
                        at: b.at,
                        last_arrival: b.last_arrival,
                        riders: b.riders.iter().map(|p| p.id).collect(),
                    }
                })
                .collect(),
        }
    }

    fn snapshot(&self) -> NetworkSnapshot {
        NetworkSnapshot::new(self.rev, self.shapes.clone(), self.routes.clone())
            .expect("sim network")
    }

    fn recompute_table(&mut self) {
        self.table = DijkstraRouter.solve(&self.snapshot());
        self.dirty = false;
    }

    fn run_commands(&mut self, events: &mut Vec<SimEvent>) {
        let cmds = std::mem::take(&mut self.cmds);
        for cmd in cmds {
            match cmd {
                Command::AddRoute { stops } => {
                    if !self.add_route(stops) {
                        events.push(SimEvent::CommandRejected);
                    }
                }
            }
        }
    }

    fn add_route(&mut self, stops: Vec<StationId>) -> bool {
        if self.free_buses == 0 {
            return false;
        }
        let slot = self
            .routes
            .iter()
            .position(|r| r.is_none())
            .unwrap_or(self.routes.len());
        if slot >= MAX_ROUTES {
            return false;
        }
        let mut trial = self.routes.clone();
        if slot == trial.len() {
            trial.push(Some(stops.clone()));
        } else {
            trial[slot] = Some(stops.clone());
        }
        if NetworkSnapshot::new(self.rev + 1, self.shapes.clone(), trial.clone()).is_err() {
            return false;
        }
        if slot == self.routes.len() {
            self.routes.push(Some(stops.clone()));
        } else {
            self.routes[slot] = Some(stops.clone());
        }
        self.rev += 1;
        self.dirty = true;
        self.free_buses -= 1;
        let route = RouteId(slot as u8);
        let at = stops[0];
        let id = BusId(self.next_bus_id);
        self.next_bus_id += 1;
        self.buses.push(Bus {
            id,
            mode: BusMode::Active {
                route,
                dir: Dir::Fwd,
            },
            phase: BusPhase::Boarding,
            at,
            last_arrival: None,
            timer: self.balance.service_ticks,
            riders: Vec::new(),
        });
        true
    }

    fn enqueue(&mut self, station: StationId, p: Passenger) {
        self.queues[usize::from(station.0)][usize::from(p.want.index())]
            .push_back((self.next_seq, p));
        self.next_seq += 1;
    }

    fn next_alighter(&self, net: &NetworkSnapshot, b: usize) -> Option<usize> {
        let bus = &self.buses[b];
        let BusMode::Active { route, dir } = bus.mode;
        let i = (0..net.stops(route)?.len()).find(|&i| net.stop(route, dir, i) == Some(bus.at))?;
        let shape = self.shapes[usize::from(bus.at.0)];
        bus.riders
            .iter()
            .position(|p| p.want == shape || !self.table.stay(p.want, route, dir)[i])
    }

    fn next_boarder(&self, b: usize) -> Option<usize> {
        let bus = &self.buses[b];
        if bus.riders.len() >= self.balance.capacity {
            return None;
        }
        let BusMode::Active { route, dir } = bus.mode;
        let bit = board_bit(route, dir);
        self.queues[usize::from(bus.at.0)]
            .iter()
            .enumerate()
            .filter_map(|(k, q)| q.front().map(|&(seq, p)| (seq, k, p)))
            .filter(|&(_, _, p)| self.table.board(p.want, bus.at) & bit != 0)
            .min_by_key(|&(seq, _, _)| seq)
            .map(|(_, k, _)| k)
    }

    fn tick_timer(&mut self, b: usize) -> bool {
        let bus = &mut self.buses[b];
        bus.timer -= 1;
        if bus.timer > 0 {
            return false;
        }
        bus.timer = self.balance.service_ticks;
        true
    }

    fn move_buses(&mut self, tick: Tick) {
        let travel = self.balance.travel_ticks;
        let service = self.balance.service_ticks;
        for bus in &mut self.buses {
            let BusPhase::Moving { from, to, progress } = bus.phase else {
                continue;
            };
            let progress = progress + 1;
            if progress >= travel {
                bus.at = to;
                bus.last_arrival = Some((to, tick));
                bus.phase = BusPhase::Alighting;
                bus.timer = service;
            } else {
                bus.phase = BusPhase::Moving { from, to, progress };
            }
        }
    }

    fn alight_buses(&mut self, events: &mut Vec<SimEvent>) {
        let net = self.snapshot();
        for b in 0..self.buses.len() {
            if !matches!(self.buses[b].phase, BusPhase::Alighting) {
                continue;
            }
            if let Some(k) = self.next_alighter(&net, b) {
                if !self.tick_timer(b) {
                    continue;
                }
                let bus = &mut self.buses[b];
                let p = bus.riders.remove(k);
                let (id, station) = (bus.id, bus.at);
                if self.shapes[usize::from(station.0)] == p.want {
                    self.score += 1;
                    events.push(SimEvent::Delivered {
                        bus: id,
                        passenger: p.id,
                        station,
                    });
                } else {
                    self.enqueue(station, p);
                    events.push(SimEvent::Alighted {
                        bus: id,
                        passenger: p.id,
                        station,
                    });
                }
                if self.next_alighter(&net, b).is_some() {
                    continue;
                }
            }
            let bus = &mut self.buses[b];
            let BusMode::Active { route, dir } = &mut bus.mode;
            if net.next_stop(*route, *dir, bus.at).is_none() {
                *dir = dir.reverse();
            }
            bus.phase = BusPhase::Boarding;
        }
    }

    fn board_buses(&mut self, events: &mut Vec<SimEvent>) {
        let net = self.snapshot();
        for b in 0..self.buses.len() {
            if !matches!(self.buses[b].phase, BusPhase::Boarding) {
                continue;
            }
            if let Some(k) = self.next_boarder(b) {
                if !self.tick_timer(b) {
                    continue;
                }
                let at = self.buses[b].at;
                let (_, p) = self.queues[usize::from(at.0)][k]
                    .pop_front()
                    .expect("eligible head");
                let bus = &mut self.buses[b];
                bus.riders.push(p);
                events.push(SimEvent::Boarded {
                    bus: bus.id,
                    passenger: p.id,
                    station: at,
                });
                if self.next_boarder(b).is_some() {
                    continue;
                }
            }
            let bus = &mut self.buses[b];
            let BusMode::Active { route, dir } = bus.mode;
            let to = net
                .next_stop(route, dir, bus.at)
                .expect("boarding has a next stop");
            bus.phase = BusPhase::Moving {
                from: bus.at,
                to,
                progress: 0,
            };
        }
    }
}
