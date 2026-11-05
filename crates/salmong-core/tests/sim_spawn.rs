use salmong_core::balance::Balance;
use salmong_core::ids::{PassengerId, StationId};
use salmong_core::rng::Pcg32;
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{Command, Sim, SimEvent};

fn scenario(ramp: &[(u32, u32)]) -> Scenario {
    weighted(ramp, [3, 5, 4])
}

fn weighted(ramp: &[(u32, u32)], w: [u32; 3]) -> Scenario {
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: ramp
            .iter()
            .map(|&(tick, rate)| RampPoint { tick, rate })
            .collect(),
        stations: [
            (12, 7, Shape::Circle, 0, w[0]),
            (30, 22, Shape::Triangle, 0, w[1]),
            (48, 12, Shape::Square, 0, w[2]),
            (50, 30, Shape::Circle, 600, 5),
        ]
        .into_iter()
        .map(|(x, y, shape, at_tick, weight)| StationSpec {
            x,
            y,
            shape,
            at_tick,
            weight,
        })
        .collect(),
    }
}

fn spawns(sim: &mut Sim, ticks: u64) -> Vec<(u64, u32, u16, Shape)> {
    let mut out = Vec::new();
    for _ in 0..ticks {
        let tick = sim.view().tick.0;
        let mut events = Vec::new();
        sim.step(&mut events);
        for e in events {
            if let SimEvent::Spawned {
                passenger: PassengerId(p),
                station: StationId(st),
                want,
            } = e
            {
                out.push((tick, p, st, want));
            }
        }
    }
    out
}

fn alive(sim: &Sim) -> usize {
    let v = sim.view();
    v.queued + v.buses.iter().map(|b| b.riders.len()).sum::<usize>()
}

use Shape::{Circle as C, Square as S, Triangle as T};

#[test]
fn pcg32_reference_vector() {
    let mut rng = Pcg32::new(42, 54);
    let out = (0..6).map(|_| rng.next_u32()).collect::<Vec<_>>();
    assert_eq!(
        out,
        [
            0xa15c02b7, 0x7b47f409, 0xba1d3330, 0x83d2f293, 0xbfa4784b, 0xcbed606e
        ]
    );
    let mut rng = Pcg32::new(42, 54);
    let big = (0..8).map(|_| rng.bounded(0x8000_0001)).collect::<Vec<_>>();
    assert_eq!(
        big,
        [
            559678134, 974992175, 64156306, 1067743306, 1273847917, 1069982636, 19922796,
            1713320025
        ]
    );
    let mut rng = Pcg32::new(7, 54);
    for _ in 0..1000 {
        assert!(rng.bounded(3) < 3);
    }
}

#[test]
fn golden_spawns_on_pack_ramp() {
    let mut sim = Sim::new(scenario(&[(0, 40), (9000, 90)]), 42, Balance::mvp());
    assert_eq!(
        spawns(&mut sim, 298),
        [
            (149, 0, 1, S),
            (187, 1, 2, T),
            (248, 2, 0, T),
            (297, 3, 1, S)
        ]
    );
}

#[test]
fn golden_spawns_hold_last_rate() {
    let mut sim = Sim::new(scenario(&[(0, 0), (10, 3000)]), 42, Balance::mvp());
    let got = spawns(&mut sim, 16);
    let ticks = got
        .iter()
        .map(|&(t, p, st, _)| (t, p, st))
        .collect::<Vec<_>>();
    assert_eq!(
        ticks,
        [
            (6, 0, 1),
            (7, 1, 2),
            (8, 2, 0),
            (9, 3, 1),
            (10, 4, 2),
            (11, 5, 1),
            (12, 6, 0),
            (12, 7, 2),
            (13, 8, 1),
            (15, 9, 0),
            (15, 10, 1),
            (15, 11, 2),
        ]
    );
    let wants = got.iter().map(|&(_, _, _, w)| w).collect::<Vec<_>>();
    assert_eq!(wants, ORACLE_HOLD_WANTS);
}

const ORACLE_HOLD_WANTS: [Shape; 12] = [S, T, T, S, T, C, S, T, C, T, C, T];

#[test]
fn want_is_another_shape_on_the_map() {
    let mut sim = Sim::new(scenario(&[(0, 100_000)]), 9, Balance::mvp());
    let got = spawns(&mut sim, 200);
    assert!(got.len() > 1000);
    let shapes = [C, T, S];
    for &(_, _, st, want) in &got {
        assert_ne!(want, shapes[usize::from(st)]);
        assert_ne!(want, Shape::Star);
    }
    for st in 0..3u16 {
        for w in shapes {
            if w != shapes[usize::from(st)] {
                assert!(got.iter().any(|&(_, _, s, x)| s == st && x == w));
            }
        }
    }
}

#[test]
fn population_never_passes_cap() {
    let mut sim = Sim::new(scenario(&[(0, 10_000_000)]), 42, Balance::mvp());
    let mut total = 0;
    for _ in 0..50 {
        total += spawns(&mut sim, 1).len();
        assert!(alive(&sim) <= 5000);
    }
    assert_eq!(alive(&sim), 5000);
    assert_eq!(total, 5000);
}

#[test]
fn spawning_resumes_below_cap() {
    let balance = Balance {
        max_alive: 2,
        ..Balance::mvp()
    };
    let mut sim = Sim::new(scenario(&[(0, 20_000)]), 42, balance);
    sim.queue(Command::AddRoute {
        stops: vec![StationId(0), StationId(1), StationId(2)],
    });
    let mut total = 0;
    for _ in 0..600 {
        total += spawns(&mut sim, 1).len();
        assert!(alive(&sim) <= 2);
    }
    assert!(sim.view().score > 0);
    assert!(total > 4);
}

#[test]
fn state_hash_covers_rng_and_accumulators() {
    let a = Sim::new(scenario(&[(0, 40)]), 42, Balance::mvp());
    let b = Sim::new(scenario(&[(0, 40)]), 43, Balance::mvp());
    assert_ne!(a.state_hash(), b.state_hash());
    let mut a = Sim::new(scenario(&[(0, 40)]), 42, Balance::mvp());
    let mut b = Sim::new(scenario(&[(0, 41)]), 42, Balance::mvp());
    assert_eq!(a.state_hash(), b.state_hash());
    assert!(spawns(&mut a, 1).is_empty() && spawns(&mut b, 1).is_empty());
    assert_ne!(a.state_hash(), b.state_hash());
}

#[test]
fn spawn_unit_boundary() {
    let mut sim = Sim::new(weighted(&[(0, 29_999)], [1, 1, 1]), 42, Balance::mvp());
    let ticks = spawns(&mut sim, 2)
        .iter()
        .map(|&(t, _, st, _)| (t, st))
        .collect::<Vec<_>>();
    assert_eq!(ticks, [(1, 0), (1, 1), (1, 2)]);
    let mut sim = Sim::new(weighted(&[(0, 30_000)], [1, 1, 1]), 42, Balance::mvp());
    assert_eq!(spawns(&mut sim, 1).len(), 3);
}

#[test]
fn paused_stations_do_not_accumulate() {
    let balance = Balance {
        max_alive: 3,
        ..Balance::mvp()
    };
    let mut a = Sim::new(scenario(&[(0, 1_000_000), (1, 1_000_000)]), 42, balance);
    let mut b = Sim::new(scenario(&[(0, 1_000_000), (1, 1_001_000)]), 42, balance);
    for _ in 0..20 {
        spawns(&mut a, 1);
        spawns(&mut b, 1);
        assert_eq!(alive(&a), 3);
        assert_eq!(a.state_hash(), b.state_hash());
    }
}
