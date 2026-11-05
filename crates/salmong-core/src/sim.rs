use crate::balance::Balance;
use crate::ids::{BusId, Dir, RouteId, StationId, Tick};
use crate::network::{MAX_ROUTES, NetworkSnapshot};
use crate::routing::{DijkstraRouter, RouteTable, Router};
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
}

#[derive(Clone, Debug)]
struct Bus {
    id: BusId,
    mode: BusMode,
    phase: BusPhase,
    at: StationId,
    last_arrival: Option<(StationId, Tick)>,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BusView {
    pub id: BusId,
    pub route: RouteId,
    pub dir: Dir,
    pub phase: BusPhase,
    pub at: StationId,
    pub last_arrival: Option<(StationId, Tick)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimView {
    pub tick: Tick,
    pub free_buses: u16,
    pub route_count: usize,
    pub table_rev: u64,
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
        }
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
        self.alight_buses();
        self.board_buses();
        self.tick = Tick(tick.0 + 1);
    }

    pub fn view(&self) -> SimView {
        SimView {
            tick: self.tick,
            free_buses: self.free_buses,
            route_count: self.routes.iter().filter(|r| r.is_some()).count(),
            table_rev: self.table.rev(),
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
        });
        true
    }

    fn move_buses(&mut self, tick: Tick) {
        let travel = self.balance.travel_ticks;
        for bus in &mut self.buses {
            let BusPhase::Moving { from, to, progress } = bus.phase else {
                continue;
            };
            let progress = progress + 1;
            if progress >= travel {
                bus.at = to;
                bus.last_arrival = Some((to, tick));
                bus.phase = BusPhase::Alighting;
            } else {
                bus.phase = BusPhase::Moving { from, to, progress };
            }
        }
    }

    fn alight_buses(&mut self) {
        let net = self.snapshot();
        for bus in &mut self.buses {
            if !matches!(bus.phase, BusPhase::Alighting) {
                continue;
            }
            let BusMode::Active { route, dir } = &mut bus.mode;
            if net.next_stop(*route, *dir, bus.at).is_none() {
                *dir = dir.reverse();
            }
            bus.phase = BusPhase::Boarding;
        }
    }

    fn board_buses(&mut self) {
        let net = self.snapshot();
        for bus in &mut self.buses {
            if !matches!(bus.phase, BusPhase::Boarding) {
                continue;
            }
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
