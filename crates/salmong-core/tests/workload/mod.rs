#![allow(dead_code)]

use salmong_core::balance::Balance;
use salmong_core::ids::{StationId, Tick};
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{Command, Sim, SimEvent};

pub const TICKS: u64 = 4_500;
pub const WARMUP: u64 = 900;
pub const EDIT_EVERY: u64 = 450;
pub const SEED: u64 = 1;
pub const HUB: StationId = StationId(63);
const STRIDES: [usize; 8] = [1, 5, 7, 11, 13, 17, 19, 23];

pub fn scenario() -> Scenario {
    Scenario {
        id: 0,
        seed: SEED as u32,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate: 100 }],
        stations: (0..64u16)
            .map(|i| StationSpec {
                x: 4 + 7 * (i % 8),
                y: 2 + 5 * (i / 8),
                shape: if StationId(i) == HUB {
                    Shape::Star
                } else {
                    Shape::ALL[usize::from(i % 3)]
                },
                at_tick: 0,
                weight: 5,
            })
            .collect(),
    }
}

pub fn stops(route: usize, hub: bool) -> Vec<StationId> {
    (0..64)
        .map(|i| StationId(((i * STRIDES[route] + route) % 64) as u16))
        .filter(|&s| hub || s != HUB)
        .collect()
}

pub fn log() -> Vec<(Tick, Command)> {
    let mut out = (0..8)
        .map(|r| {
            (
                Tick(0),
                Command::AddRoute {
                    stops: stops(r, false),
                },
            )
        })
        .collect::<Vec<_>>();
    let mut hub = true;
    let mut t = WARMUP;
    while t < TICKS {
        out.push((
            Tick(t),
            Command::EditRoute {
                route: salmong_core::ids::RouteId(7),
                stops: stops(7, hub),
            },
        ));
        hub = !hub;
        t += EDIT_EVERY;
    }
    out
}

pub fn run(balance: Balance, mut step: impl FnMut(u64, &mut Sim, &mut Vec<SimEvent>)) -> Sim {
    let mut sim = Sim::new(scenario(), SEED, balance);
    let log = log();
    let mut next = 0;
    let mut events = Vec::new();
    for t in 0..TICKS {
        while next < log.len() && log[next].0.0 == t {
            sim.queue(log[next].1.clone());
            next += 1;
        }
        events.clear();
        step(t, &mut sim, &mut events);
    }
    sim
}

#[derive(Default)]
pub struct Population {
    pub alive: usize,
    pub peak: usize,
    pub cap_tick: Option<u64>,
}

impl Population {
    pub fn observe(&mut self, tick: u64, events: &[SimEvent], cap: usize) {
        for e in events {
            match e {
                SimEvent::Spawned { .. } => self.alive += 1,
                SimEvent::Delivered { .. } => self.alive -= 1,
                _ => {}
            }
            self.peak = self.peak.max(self.alive);
            if self.alive >= cap && self.cap_tick.is_none() {
                self.cap_tick = Some(tick);
            }
        }
    }
}
