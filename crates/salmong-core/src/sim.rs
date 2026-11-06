use crate::balance::Balance;
use std::collections::VecDeque;

use crate::ids::{BusId, Dir, PassengerId, RouteId, StationId, Tick};
use crate::network::{MAX_ROUTES, NetworkSnapshot};
use crate::rng::Pcg32;
use crate::routing::{DijkstraRouter, RouteTable, Router, board_bit};
use crate::scenario::{RampPoint, Scenario, Shape};
use crate::score::FinalScore;
use crate::spawn::rate;
use crate::trace::Action;
#[cfg(feature = "trace")]
use crate::trace::{BusState, Phase, TraceState, TraceStep};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusMode {
    Active { route: RouteId, dir: Dir },
    Retiring,
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
    AddRoute {
        stops: Vec<StationId>,
    },
    EditRoute {
        route: RouteId,
        stops: Vec<StationId>,
    },
    RemoveRoute {
        route: RouteId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimMode {
    Running,
    Over,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimEvent {
    CommandRejected,
    OverflowStarted {
        station: StationId,
    },
    GameOver,
    Spawned {
        passenger: PassengerId,
        station: StationId,
        want: Shape,
    },
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
    pos: Vec<(i64, i64)>,
    weights: Vec<u64>,
    ramp: Vec<RampPoint>,
    acc: Vec<u64>,
    rng: Pcg32,
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
    overflow: Vec<u32>,
    mode: SimMode,
    #[cfg(feature = "trace")]
    trace: Option<Recorder>,
}

#[cfg(feature = "trace")]
struct Recorder {
    last: TraceState,
    steps: Vec<TraceStep>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BusView {
    pub id: BusId,
    pub mode: BusMode,
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
    pub mode: SimMode,
    pub overflow: Vec<u32>,
    pub queues: Vec<Vec<Passenger>>,
    pub buses: Vec<BusView>,
}

impl Sim {
    pub fn new(scenario: Scenario, seed: u64, balance: Balance) -> Self {
        let start = scenario.stations.iter().filter(|s| s.at_tick == 0);
        let shapes = start.clone().map(|s| s.shape).collect::<Vec<_>>();
        let weights = start
            .clone()
            .map(|s| u64::from(s.weight))
            .collect::<Vec<_>>();
        let pos = start.map(|s| (i64::from(s.x), i64::from(s.y))).collect();
        let net = NetworkSnapshot::new(0, shapes.clone(), vec![]).expect("start stations");
        let table = DijkstraRouter.solve(&net);
        let queues = shapes.iter().map(|_| Default::default()).collect();
        let overflow = vec![0; shapes.len()];
        Self {
            balance,
            shapes,
            pos,
            acc: vec![0; weights.len()],
            weights,
            ramp: scenario.ramp,
            rng: Pcg32::new(seed, 54),
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
            overflow,
            mode: SimMode::Running,
            #[cfg(feature = "trace")]
            trace: None,
        }
    }

    #[cfg(feature = "fixtures")]
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
        self.run(events, |_, _| {});
    }

    #[cfg(feature = "fixtures")]
    pub fn step_observed(
        &mut self,
        events: &mut Vec<SimEvent>,
        check: impl FnMut(&Sim, &[SimEvent]),
    ) {
        self.run(events, check);
    }

    fn run(&mut self, events: &mut Vec<SimEvent>, mut check: impl FnMut(&Sim, &[SimEvent])) {
        if self.mode == SimMode::Over {
            return;
        }
        let tick = self.tick;
        self.run_commands(events);
        check(self, events);
        if self.dirty {
            self.recompute_table();
        }
        check(self, events);
        self.spawn(tick, events);
        check(self, events);
        self.move_buses(tick);
        check(self, events);
        let net = self.snapshot();
        for b in 0..self.buses.len() {
            self.alight_bus(&net, b, events);
            check(self, events);
        }
        for b in 0..self.buses.len() {
            self.board_bus(&net, b, events);
            check(self, events);
        }
        self.park_retired();
        check(self, events);
        self.overflow(events);
        self.tick = Tick(tick.0 + 1);
        check(self, events);
    }

    fn overflow(&mut self, events: &mut Vec<SimEvent>) {
        for st in 0..self.shapes.len() {
            let queue = self.queues[st].iter().map(VecDeque::len).sum::<usize>();
            let timer = &mut self.overflow[st];
            if queue > self.balance.station_capacity {
                if *timer == 0 {
                    events.push(SimEvent::OverflowStarted {
                        station: StationId(st as u16),
                    });
                }
                *timer += 1;
            } else {
                *timer = timer.saturating_sub(2);
            }
        }
        if self
            .overflow
            .iter()
            .any(|&t| t >= self.balance.overflow_limit)
        {
            self.mode = SimMode::Over;
            events.push(SimEvent::GameOver);
        }
    }

    pub fn final_score(&self) -> Option<FinalScore> {
        (self.mode == SimMode::Over).then_some(FinalScore {
            score: self.score,
            ticks: self.tick.0,
        })
    }

    pub fn view(&self) -> SimView {
        SimView {
            tick: self.tick,
            free_buses: self.free_buses,
            route_count: self.routes.iter().filter(|r| r.is_some()).count(),
            table_rev: self.table.rev(),
            score: self.score,
            queued: self.queues.iter().flatten().map(VecDeque::len).sum(),
            mode: self.mode,
            overflow: self.overflow.clone(),
            queues: self
                .queues
                .iter()
                .map(|qs| qs.iter().flatten().map(|&(_, p)| p).collect())
                .collect(),
            buses: self
                .buses
                .iter()
                .map(|b| BusView {
                    id: b.id,
                    mode: b.mode.clone(),
                    phase: b.phase.clone(),
                    at: b.at,
                    last_arrival: b.last_arrival,
                    riders: b.riders.iter().map(|p| p.id).collect(),
                })
                .collect(),
        }
    }

    fn alive(&self) -> usize {
        let queued = self
            .queues
            .iter()
            .flatten()
            .map(VecDeque::len)
            .sum::<usize>();
        queued + self.buses.iter().map(|b| b.riders.len()).sum::<usize>()
    }

    fn spawn(&mut self, tick: Tick, events: &mut Vec<SimEvent>) {
        let rate = rate(&self.ramp, tick.0);
        let mut alive = self.alive();
        for st in 0..self.shapes.len() {
            if alive >= self.balance.max_alive {
                return;
            }
            self.acc[st] += self.weights[st] * rate;
            while self.acc[st] >= self.balance.spawn_unit && alive < self.balance.max_alive {
                self.acc[st] -= self.balance.spawn_unit;
                let own = self.shapes[st];
                let eligible = Shape::ALL
                    .into_iter()
                    .filter(|&s| s != own && self.shapes.contains(&s))
                    .collect::<Vec<_>>();
                let want = eligible[self.rng.bounded(eligible.len() as u32) as usize];
                let id = PassengerId(self.next_passenger);
                self.next_passenger += 1;
                let station = StationId(st as u16);
                self.enqueue(station, Passenger { id, want });
                self.note(Action::Spawn {
                    passenger: id,
                    want,
                });
                alive += 1;
                events.push(SimEvent::Spawned {
                    passenger: id,
                    station,
                    want,
                });
            }
        }
    }

    pub fn snapshot(&self) -> NetworkSnapshot {
        NetworkSnapshot::new(self.rev, self.shapes.clone(), self.routes.clone())
            .expect("sim network")
    }

    fn recompute_table(&mut self) {
        self.table = DijkstraRouter.solve(&self.snapshot());
        self.dirty = false;
        let service = self.balance.service_ticks;
        for bus in &mut self.buses {
            let stopped = matches!(bus.phase, BusPhase::Alighting | BusPhase::Boarding);
            if stopped && matches!(bus.mode, BusMode::Active { .. }) {
                bus.phase = BusPhase::Alighting;
                bus.timer = service;
            }
        }
        self.note(Action::EditNetwork);
    }

    pub fn state_bytes(&self) -> Vec<u8> {
        let mut w = Vec::new();
        let u = |w: &mut Vec<u8>, v: u64| w.extend_from_slice(&v.to_le_bytes());
        u(&mut w, self.tick.0);
        u(&mut w, self.rev);
        u(&mut w, u64::from(self.dirty));
        u(&mut w, u64::from(self.free_buses));
        u(&mut w, u64::from(self.next_bus_id));
        u(&mut w, self.next_seq);
        u(&mut w, u64::from(self.next_passenger));
        u(&mut w, self.score);
        let (rs, ri) = self.rng.state();
        u(&mut w, rs);
        u(&mut w, ri);
        for a in &self.acc {
            u(&mut w, *a);
        }
        u(&mut w, u64::from(self.mode == SimMode::Over));
        for t in &self.overflow {
            u(&mut w, u64::from(*t));
        }
        u(&mut w, self.routes.len() as u64);
        for r in &self.routes {
            let stops = r.as_deref().unwrap_or(&[]);
            u(&mut w, u64::from(r.is_some()));
            u(&mut w, stops.len() as u64);
            for s in stops {
                u(&mut w, u64::from(s.0));
            }
        }
        u(&mut w, self.buses.len() as u64);
        for b in &self.buses {
            u(&mut w, u64::from(b.id.0));
            match b.mode {
                BusMode::Active { route, dir } => {
                    u(&mut w, 1);
                    u(&mut w, u64::from(route.0));
                    u(&mut w, dir.index() as u64);
                }
                BusMode::Retiring => u(&mut w, 2),
            }
            match b.phase {
                BusPhase::Moving { from, to, progress } => {
                    u(&mut w, 1);
                    u(&mut w, u64::from(from.0));
                    u(&mut w, u64::from(to.0));
                    u(&mut w, u64::from(progress));
                }
                BusPhase::Alighting => u(&mut w, 2),
                BusPhase::Boarding => u(&mut w, 3),
            }
            u(&mut w, u64::from(b.at.0));
            let (st, t) = b
                .last_arrival
                .map_or((u64::MAX, u64::MAX), |(s, t)| (u64::from(s.0), t.0));
            u(&mut w, st);
            u(&mut w, t);
            u(&mut w, u64::from(b.timer));
            u(&mut w, b.riders.len() as u64);
            for p in &b.riders {
                u(&mut w, u64::from(p.id.0));
                u(&mut w, u64::from(p.want.index()));
            }
        }
        for q in self.queues.iter().flatten() {
            u(&mut w, q.len() as u64);
            for (seq, p) in q {
                u(&mut w, *seq);
                u(&mut w, u64::from(p.id.0));
            }
        }
        u(&mut w, self.cmds.len() as u64);
        for c in &self.cmds {
            let (tag, route, stops) = match c {
                Command::AddRoute { stops } => (1, 0, stops.as_slice()),
                Command::EditRoute { route, stops } => (2, route.0, stops.as_slice()),
                Command::RemoveRoute { route } => (3, route.0, &[][..]),
            };
            u(&mut w, tag);
            u(&mut w, u64::from(route));
            u(&mut w, stops.len() as u64);
            for s in stops {
                u(&mut w, u64::from(s.0));
            }
        }
        w
    }

    pub fn state_hash(&self) -> u64 {
        self.state_bytes()
            .iter()
            .fold(0xcbf2_9ce4_8422_2325, |h, &b| {
                (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
            })
    }

    fn run_commands(&mut self, events: &mut Vec<SimEvent>) {
        let cmds = std::mem::take(&mut self.cmds);
        for cmd in cmds {
            let ok = match cmd {
                Command::AddRoute { stops } => self.add_route(stops),
                Command::EditRoute { route, stops } => self.set_route(route, Some(stops)),
                Command::RemoveRoute { route } => self.set_route(route, None),
            };
            if !ok {
                events.push(SimEvent::CommandRejected);
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

    fn set_route(&mut self, route: RouteId, stops: Option<Vec<StationId>>) -> bool {
        let slot = usize::from(route.0);
        if !matches!(self.routes.get(slot), Some(Some(_))) {
            return false;
        }
        let mut trial = self.routes.clone();
        trial[slot] = stops.clone();
        if NetworkSnapshot::new(self.rev + 1, self.shapes.clone(), trial).is_err() {
            return false;
        }
        let removed = stops.is_none();
        self.routes[slot] = stops;
        self.rev += 1;
        self.dirty = true;
        if removed {
            let service = self.balance.service_ticks;
            for bus in &mut self.buses {
                if !matches!(bus.mode, BusMode::Active { route: r, .. } if r == route) {
                    continue;
                }
                bus.mode = BusMode::Retiring;
                if bus.phase == BusPhase::Boarding {
                    bus.phase = BusPhase::Alighting;
                    bus.timer = service;
                }
            }
        }
        true
    }

    fn enqueue(&mut self, station: StationId, p: Passenger) {
        self.queues[usize::from(station.0)][usize::from(p.want.index())]
            .push_back((self.next_seq, p));
        self.next_seq += 1;
    }

    fn next_alighter(&self, net: &NetworkSnapshot, b: usize) -> Option<usize> {
        let bus = &self.buses[b];
        let Some((route, dir, i)) = Self::stop_index(net, bus) else {
            return (!bus.riders.is_empty()).then_some(0);
        };
        let shape = self.shapes[usize::from(bus.at.0)];
        bus.riders
            .iter()
            .position(|p| p.want == shape || !self.table.stay(p.want, route, dir)[i])
    }

    fn stop_index(net: &NetworkSnapshot, bus: &Bus) -> Option<(RouteId, Dir, usize)> {
        let BusMode::Active { route, dir } = bus.mode else {
            return None;
        };
        let i = (0..net.stops(route)?.len()).find(|&i| net.stop(route, dir, i) == Some(bus.at))?;
        Some((route, dir, i))
    }

    fn next_boarder(&self, b: usize) -> Option<usize> {
        let bus = &self.buses[b];
        if bus.riders.len() >= self.balance.capacity {
            return None;
        }
        let BusMode::Active { route, dir } = bus.mode else {
            return None;
        };
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
        for b in 0..self.buses.len() {
            let bus = &mut self.buses[b];
            let BusPhase::Moving { from, to, progress } = bus.phase else {
                continue;
            };
            let progress = progress + 1;
            if progress >= travel {
                bus.at = to;
                bus.last_arrival = Some((to, tick));
                bus.phase = BusPhase::Alighting;
                bus.timer = service;
                let id = bus.id;
                self.note(Action::Move { bus: id });
            } else {
                bus.phase = BusPhase::Moving { from, to, progress };
            }
        }
    }

    fn alight_bus(&mut self, net: &NetworkSnapshot, b: usize, events: &mut Vec<SimEvent>) {
        if !matches!(self.buses[b].phase, BusPhase::Alighting) {
            return;
        }
        if let Some(k) = self.next_alighter(net, b) {
            if !self.tick_timer(b) {
                return;
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
            self.note(Action::Alight {
                bus: id,
                passenger: p.id,
            });
            if self.next_alighter(net, b).is_some() {
                return;
            }
        }
        let on_route = Self::stop_index(net, &self.buses[b]).is_some();
        let bus = &mut self.buses[b];
        let id = bus.id;
        let BusMode::Active { route, dir } = &mut bus.mode else {
            bus.phase = BusPhase::Boarding;
            self.note(Action::EndAlight { bus: id });
            return;
        };
        if on_route {
            if net.next_stop(*route, *dir, bus.at).is_none() {
                *dir = dir.reverse();
            }
            bus.phase = BusPhase::Boarding;
            self.note(Action::EndAlight { bus: id });
            return;
        }
        let stops = net.stops(*route).expect("active route");
        let (x, y) = self.pos[usize::from(bus.at.0)];
        let d2 = |s: &StationId| {
            let (sx, sy) = self.pos[usize::from(s.0)];
            (sx - x).pow(2) + (sy - y).pow(2)
        };
        let i = (0..stops.len())
            .min_by_key(|&i| (d2(&stops[i]), i))
            .expect("route has stops");
        *dir = if i + 1 == stops.len() {
            Dir::Back
        } else {
            Dir::Fwd
        };
        bus.phase = BusPhase::Moving {
            from: bus.at,
            to: stops[i],
            progress: 0,
        };
        self.note(Action::EndAlight { bus: id });
    }

    fn board_bus(&mut self, net: &NetworkSnapshot, b: usize, events: &mut Vec<SimEvent>) {
        let bus = &self.buses[b];
        if bus.phase != BusPhase::Boarding || bus.mode == BusMode::Retiring {
            return;
        }
        if let Some(k) = self.next_boarder(b) {
            if !self.tick_timer(b) {
                return;
            }
            let at = self.buses[b].at;
            let (_, p) = self.queues[usize::from(at.0)][k]
                .pop_front()
                .expect("eligible head");
            let bus = &mut self.buses[b];
            bus.riders.push(p);
            let id = bus.id;
            events.push(SimEvent::Boarded {
                bus: id,
                passenger: p.id,
                station: at,
            });
            self.note(Action::Board {
                bus: id,
                passenger: p.id,
            });
            if self.next_boarder(b).is_some() {
                return;
            }
        }
        let bus = &mut self.buses[b];
        let BusMode::Active { route, dir } = bus.mode else {
            return;
        };
        let to = net
            .next_stop(route, dir, bus.at)
            .expect("boarding has a next stop");
        bus.phase = BusPhase::Moving {
            from: bus.at,
            to,
            progress: 0,
        };
        let id = bus.id;
        self.note(Action::Depart { bus: id });
    }

    fn park_retired(&mut self) {
        let before = self.buses.len();
        self.buses
            .retain(|b| !(b.mode == BusMode::Retiring && b.phase == BusPhase::Boarding));
        self.free_buses += (before - self.buses.len()) as u16;
    }

    #[cfg(not(feature = "trace"))]
    fn note(&mut self, _: Action) {}

    #[cfg(feature = "trace")]
    fn note(&mut self, action: Action) {
        let Some(mut rec) = self.trace.take() else {
            return;
        };
        let after = self.project(&rec.last, action);
        let before = std::mem::replace(&mut rec.last, after.clone());
        rec.steps.push(TraceStep {
            tick: self.tick,
            action,
            before,
            after,
        });
        self.trace = Some(rec);
    }

    #[cfg(feature = "trace")]
    pub fn start_trace(&mut self) {
        let wants = (0..self.next_passenger)
            .filter_map(|id| self.want_of(PassengerId(id)))
            .collect();
        let base = TraceState {
            network: Vec::new(),
            buses: Vec::new(),
            queues: Vec::new(),
            delivered: Vec::new(),
            edits: 0,
            wants,
        };
        let last = self.project(&base, Action::EditNetwork);
        let last = TraceState { edits: 0, ..last };
        self.trace = Some(Recorder {
            last,
            steps: Vec::new(),
        });
    }

    #[cfg(feature = "trace")]
    pub fn take_trace(&mut self) -> Vec<TraceStep> {
        self.trace
            .as_mut()
            .map(|rec| std::mem::take(&mut rec.steps))
            .unwrap_or_default()
    }

    #[cfg(feature = "trace")]
    fn want_of(&self, id: PassengerId) -> Option<Shape> {
        let queued = self.queues.iter().flatten().flatten().map(|&(_, p)| p);
        let riding = self.buses.iter().flat_map(|b| b.riders.iter().copied());
        queued.chain(riding).find(|p| p.id == id).map(|p| p.want)
    }

    #[cfg(feature = "trace")]
    fn project(&self, prev: &TraceState, action: Action) -> TraceState {
        let slots = self
            .buses
            .iter()
            .map(|b| usize::from(b.id.0) + 1)
            .max()
            .unwrap_or(0)
            .max(prev.buses.len());
        let buses = (0..slots)
            .filter_map(|i| {
                let Some(bus) = self.buses.iter().find(|b| usize::from(b.id.0) == i) else {
                    return prev.buses.get(i).cloned();
                };
                let (dir, retiring) = match bus.mode {
                    BusMode::Active { dir, .. } => (dir, false),
                    BusMode::Retiring => (prev.buses.get(i).map_or(Dir::Fwd, |b| b.dir), true),
                };
                let (phase, at) = match bus.phase {
                    BusPhase::Moving { to, .. } => (Phase::Moving, to),
                    BusPhase::Alighting => (Phase::Alighting, bus.at),
                    BusPhase::Boarding if retiring => (Phase::Parked, bus.at),
                    BusPhase::Boarding => (Phase::Boarding, bus.at),
                };
                Some(BusState {
                    at,
                    dir,
                    phase,
                    retiring,
                    riders: bus.riders.iter().map(|p| p.id).collect(),
                })
            })
            .collect::<Vec<_>>();
        let queues = self
            .queues
            .iter()
            .map(|qs| {
                let mut q = qs.iter().flatten().copied().collect::<Vec<_>>();
                q.sort_by_key(|&(seq, _)| seq);
                q.into_iter().map(|(_, p)| p.id).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let delivered = (0..self.next_passenger)
            .map(PassengerId)
            .filter(|id| {
                !queues.iter().flatten().any(|p| p == id)
                    && !self
                        .buses
                        .iter()
                        .flat_map(|b| &b.riders)
                        .any(|p| p.id == *id)
            })
            .collect();
        let mut wants = prev.wants.clone();
        let mut edits = prev.edits;
        match action {
            Action::Spawn { want, .. } => wants.push(want),
            Action::EditNetwork => edits += 1,
            _ => {}
        }
        TraceState {
            network: self
                .routes
                .iter()
                .map(|r| r.clone().unwrap_or_default())
                .collect(),
            buses,
            queues,
            delivered,
            edits,
            wants,
        }
    }
}
