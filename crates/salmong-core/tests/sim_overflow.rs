use salmong_core::balance::Balance;
use salmong_core::ids::{PassengerId, RouteId, StationId};
use salmong_core::rng::Pcg32;
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{Command, Sim, SimEvent, SimMode};
use std::collections::BTreeSet;

fn scenario(rate: u32) -> Scenario {
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate }],
        stations: [
            (12, 7, Shape::Circle),
            (30, 22, Shape::Triangle),
            (48, 12, Shape::Square),
        ]
        .into_iter()
        .map(|(x, y, shape)| StationSpec {
            x,
            y,
            shape,
            at_tick: 0,
            weight: 3,
        })
        .collect(),
    }
}

fn placed(n: usize) -> Sim {
    let mut sim = Sim::new(scenario(0), 0, Balance::mvp());
    for _ in 0..n {
        sim.place_passenger(StationId(0), Shape::Triangle);
    }
    sim
}

fn step(sim: &mut Sim) -> Vec<SimEvent> {
    let mut events = Vec::new();
    sim.step(&mut events);
    events
}

#[test]
fn game_over_on_exactly_tick_1350_of_overflow() {
    let mut sim = placed(9);
    let mut log = Vec::new();
    for t in 0..1349u64 {
        for e in step(&mut sim) {
            log.push((t, e));
        }
        assert_eq!(sim.view().overflow, vec![t as u32 + 1, 0, 0]);
        assert_eq!(sim.view().mode, SimMode::Running);
    }
    assert_eq!(
        log,
        vec![(
            0,
            SimEvent::OverflowStarted {
                station: StationId(0)
            }
        )]
    );
    assert_eq!(step(&mut sim), vec![SimEvent::GameOver]);
    let view = sim.view();
    assert_eq!(view.tick.0, 1350);
    assert_eq!(view.mode, SimMode::Over);
    assert_eq!(view.overflow, vec![1350, 0, 0]);
}

#[test]
fn over_freezes_the_sim() {
    let mut sim = placed(9);
    for _ in 0..1350 {
        step(&mut sim);
    }
    let hash = sim.state_hash();
    sim.queue(Command::AddRoute {
        stops: vec![StationId(0), StationId(1)],
    });
    let queued = sim.state_hash();
    assert_ne!(hash, queued);
    for _ in 0..5 {
        assert!(step(&mut sim).is_empty());
    }
    assert_eq!(sim.state_hash(), queued);
    assert_eq!(sim.view().tick.0, 1350);
}

#[test]
fn queue_at_capacity_does_not_overflow() {
    let mut sim = placed(8);
    for _ in 0..100 {
        assert!(step(&mut sim).is_empty());
        assert_eq!(sim.view().overflow, vec![0, 0, 0]);
    }
}

#[test]
fn overflow_counts_every_wanted_shape() {
    let mut sim = Sim::new(scenario(0), 0, Balance::mvp());
    for want in [Shape::Triangle, Shape::Square, Shape::Triangle] {
        for _ in 0..3 {
            sim.place_passenger(StationId(0), want);
        }
    }
    step(&mut sim);
    assert_eq!(sim.view().overflow, vec![1, 0, 0]);
}

#[test]
fn decay_of_2_per_tick() {
    let mut sim = placed(9);
    let mut series = Vec::new();
    for t in 0..27u64 {
        if t == 10 {
            sim.queue(Command::AddRoute {
                stops: vec![StationId(0), StationId(1)],
            });
        }
        step(&mut sim);
        series.push(sim.view().overflow[0]);
    }
    let mut expected = (1..=17).collect::<Vec<u32>>();
    expected.extend([15, 13, 11, 9, 7, 5, 3, 1, 0, 0]);
    assert_eq!(series, expected);
    assert_eq!(sim.view().queued, 7);
}

#[test]
fn state_hash_covers_overflow_and_mode() {
    let mut a = placed(9);
    let mut b = placed(9);
    step(&mut a);
    assert_ne!(a.state_hash(), b.state_hash());
    step(&mut b);
    assert_eq!(a.state_hash(), b.state_hash());
    let balance = Balance {
        overflow_limit: 2,
        ..Balance::mvp()
    };
    let mut c = Sim::new(scenario(0), 0, balance);
    let mut d = Sim::new(scenario(0), 0, Balance::mvp());
    for sim in [&mut c, &mut d] {
        for _ in 0..9 {
            sim.place_passenger(StationId(0), Shape::Triangle);
        }
        step(sim);
        step(sim);
    }
    assert_eq!(c.view().mode, SimMode::Over);
    assert_ne!(c.state_hash(), d.state_hash());
}

#[test]
fn state_hash_covers_overflow_alone() {
    let mut a = placed(9);
    let balance = Balance {
        station_capacity: 9,
        ..Balance::mvp()
    };
    let mut b = Sim::new(scenario(0), 0, balance);
    for _ in 0..9 {
        b.place_passenger(StationId(0), Shape::Triangle);
    }
    step(&mut a);
    step(&mut b);
    assert_eq!(a.view().overflow, vec![1, 0, 0]);
    assert_eq!(b.view().overflow, vec![0, 0, 0]);
    assert_ne!(a.state_hash(), b.state_hash());
}

#[test]
fn microstep_checks_cover_every_phase_and_bus() {
    let mut sim = placed(9);
    sim.queue(Command::AddRoute {
        stops: vec![StationId(0), StationId(1)],
    });
    sim.queue(Command::AddRoute {
        stops: vec![StationId(1), StationId(2)],
    });
    for _ in 0..3 {
        let mut calls = 0;
        sim.step_observed(&mut Vec::new(), |_, _| calls += 1);
        assert_eq!(calls, 6 + 2 * 2);
    }
}

fn random_command(rng: &mut Pcg32) -> Command {
    let mut stops = vec![StationId(0), StationId(1), StationId(2)];
    for i in (1..3).rev() {
        stops.swap(i, rng.bounded(i as u32 + 1) as usize);
    }
    stops.truncate(2 + rng.bounded(2) as usize);
    let route = RouteId(rng.bounded(4) as u8);
    match rng.bounded(4) {
        0 | 1 => Command::AddRoute { stops },
        2 => Command::EditRoute { route, stops },
        _ => Command::RemoveRoute { route },
    }
}

fn check(sim: &Sim, events: &[SimEvent], spawned_before: usize, balance: Balance) {
    let view = sim.view();
    let shapes = [Shape::Circle, Shape::Triangle, Shape::Square];
    let mut ids = BTreeSet::new();
    for (st, q) in view.queues.iter().enumerate() {
        for p in q {
            assert_ne!(p.want, shapes[st], "queued at its own shape");
            assert!(ids.insert(p.id), "duplicate {:?}", p.id);
        }
    }
    for bus in &view.buses {
        assert!(bus.riders.len() <= balance.capacity, "over capacity");
        for &PassengerId(p) in &bus.riders {
            assert!(ids.insert(PassengerId(p)), "duplicate rider {p}");
        }
    }
    assert_eq!(
        ids.len(),
        view.queued + view.buses.iter().map(|b| b.riders.len()).sum::<usize>()
    );
    let spawned = spawned_before
        + events
            .iter()
            .filter(|e| matches!(e, SimEvent::Spawned { .. }))
            .count();
    assert_eq!(
        spawned as u64,
        ids.len() as u64 + view.score,
        "conservation"
    );
    assert!(ids.len() <= balance.max_alive);
    assert!(view.overflow.iter().all(|&t| t <= balance.overflow_limit));
    let reached = view.overflow.iter().any(|&t| t >= balance.overflow_limit);
    assert_eq!(view.mode == SimMode::Over, reached, "mode matches timers");
}

#[test]
fn safety_holds_after_every_microstep() {
    let balance = Balance {
        overflow_limit: 300,
        ..Balance::mvp()
    };
    let (mut overs, mut decays, mut microsteps) = (0, 0, 0u64);
    for seed in 0..64u64 {
        let mut rng = Pcg32::new(seed, 7);
        let mut sim = Sim::new(scenario(40 * seed as u32), seed, balance);
        let mut spawned = 0;
        let mut prev = vec![0; 3];
        for _ in 0..3_000 {
            if rng.bounded(15) == 0 {
                sim.queue(random_command(&mut rng));
            }
            let mut events = Vec::new();
            sim.step_observed(&mut events, |s, ev| {
                check(s, ev, spawned, balance);
                microsteps += 1;
            });
            spawned += events
                .iter()
                .filter(|e| matches!(e, SimEvent::Spawned { .. }))
                .count();
            let now = sim.view().overflow;
            if now.iter().zip(&prev).any(|(n, p)| n < p) {
                decays += 1;
            }
            prev = now;
            if sim.view().mode == SimMode::Over {
                overs += 1;
                break;
            }
        }
    }
    assert!(overs > 0 && overs < 64, "overs {overs}");
    eprintln!("overs {overs} decays {decays} microsteps {microsteps}");
    assert!(decays > 0);
    assert!(microsteps > 100_000, "microsteps {microsteps}");
}
