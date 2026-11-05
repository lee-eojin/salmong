use salmong_core::balance::Balance;
use salmong_core::ids::{BusId, PassengerId, StationId};
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{BusPhase, Command, Sim, SimEvent};

fn scenario() -> Scenario {
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate: 0 }],
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

fn add(sim: &mut Sim, route: &[u16]) {
    sim.queue(Command::AddRoute {
        stops: route.iter().map(|&i| StationId(i)).collect(),
    });
}

#[derive(Debug, PartialEq, Eq)]
enum Ev {
    B(u64, u16, u32, u16),
    A(u64, u16, u32, u16),
    D(u64, u16, u32, u16),
}

fn log(sim: &mut Sim, ticks: u64, mut before: impl FnMut(&mut Sim, u64)) -> Vec<Ev> {
    let mut out = Vec::new();
    for _ in 0..ticks {
        let t = sim.view().tick.0;
        before(sim, t);
        let mut events = Vec::new();
        sim.step(&mut events);
        for e in events {
            out.push(match e {
                SimEvent::Boarded {
                    bus: BusId(b),
                    passenger: PassengerId(p),
                    station: StationId(s),
                } => Ev::B(t, b, p, s),
                SimEvent::Alighted {
                    bus: BusId(b),
                    passenger: PassengerId(p),
                    station: StationId(s),
                } => Ev::A(t, b, p, s),
                SimEvent::Delivered {
                    bus: BusId(b),
                    passenger: PassengerId(p),
                    station: StationId(s),
                } => Ev::D(t, b, p, s),
                SimEvent::CommandRejected => panic!("unexpected reject at {t}"),
                SimEvent::Spawned { .. } => panic!("unexpected spawn at {t}"),
                SimEvent::OverflowStarted { .. } | SimEvent::GameOver => continue,
            });
        }
    }
    out
}

fn place(sim: &mut Sim, st: u16, want: Shape, n: usize) {
    for _ in 0..n {
        sim.place_passenger(StationId(st), want);
    }
}

#[test]
fn competing_buses_lower_bus_id_wins() {
    let mut sim = Sim::new(scenario(), 0, Balance::mvp());
    add(&mut sim, &[0, 1]);
    add(&mut sim, &[0, 1]);
    place(&mut sim, 0, Shape::Triangle, 3);
    let got = log(&mut sim, 50, |_, _| {});
    use Ev::*;
    assert_eq!(
        got,
        vec![
            B(7, 0, 0, 0),
            B(7, 1, 1, 0),
            B(15, 0, 2, 0),
            D(32, 0, 0, 1),
            D(32, 1, 1, 1),
            D(40, 0, 2, 1),
        ]
    );
    assert_eq!(sim.view().score, 3);
}

#[test]
fn transfer_boards_another_bus_in_the_same_tick() {
    let mut sim = Sim::new(scenario(), 0, Balance::mvp());
    add(&mut sim, &[1, 2]);
    add(&mut sim, &[0, 1]);
    place(&mut sim, 0, Shape::Square, 1);
    place(&mut sim, 0, Shape::Triangle, 2);
    let got = log(&mut sim, 70, |_, _| {});
    use Ev::*;
    assert_eq!(
        got,
        vec![
            B(7, 1, 0, 0),
            B(15, 1, 1, 0),
            B(23, 1, 2, 0),
            A(40, 1, 0, 1),
            B(47, 0, 0, 1),
            D(48, 1, 1, 1),
            D(56, 1, 2, 1),
            D(64, 0, 0, 2),
        ]
    );
    assert_eq!(sim.view().score, 3);
}

#[test]
fn sustained_arrivals_full_bus_departs() {
    let mut sim = Sim::new(scenario(), 0, Balance::mvp());
    add(&mut sim, &[0, 1]);
    let got = log(&mut sim, 171, |s, _| place(s, 0, Shape::Triangle, 1));
    use Ev::*;
    let mut want = Vec::new();
    for (k, t) in [7, 15, 23, 31, 39, 47].into_iter().enumerate() {
        want.push(B(t, 0, k as u32, 0));
    }
    for (k, t) in [64, 72, 80, 88, 96, 104].into_iter().enumerate() {
        want.push(D(t, 0, k as u32, 1));
    }
    for (k, t) in [121, 129, 137, 145, 153, 161].into_iter().enumerate() {
        want.push(B(t, 0, 6 + k as u32, 0));
    }
    assert_eq!(got, want);
    assert_eq!(sim.view().queued, 171 - 12);
}

#[test]
fn boarding_dwell_is_bounded_by_free_seats() {
    let cap = Balance::mvp().capacity as u64;
    let service = u64::from(Balance::mvp().service_ticks);
    let balance = Balance {
        overflow_limit: u32::MAX,
        ..Balance::mvp()
    };
    let mut sim = Sim::new(scenario(), 0, balance);
    add(&mut sim, &[0, 1, 2]);
    add(&mut sim, &[2, 0]);
    let wants = [Shape::Triangle, Shape::Square, Shape::Circle];
    let mut start: Vec<Option<(u64, u64)>> = vec![None; 2];
    let mut phases = 0;
    for t in 0..3000u64 {
        if t % 3 == 0 {
            let st = (t / 3 % 3) as u16;
            sim.place_passenger(StationId(st), wants[((t / 9 + u64::from(st)) % 3) as usize]);
        }
        let mut events = Vec::new();
        sim.step(&mut events);
        for (b, bus) in sim.view().buses.iter().enumerate() {
            match (&bus.phase, start[b]) {
                (BusPhase::Boarding, None) => {
                    start[b] = Some((t, cap - bus.riders.len() as u64));
                }
                (BusPhase::Moving { progress: 0, .. }, s) => {
                    let (s0, free) = s.unwrap_or((t, cap - bus.riders.len() as u64));
                    assert!(
                        t - s0 <= free * service,
                        "bus {b} dwell {} free {free}",
                        t - s0
                    );
                    start[b] = None;
                    phases += 1;
                }
                _ => {}
            }
        }
    }
    assert!(phases > 50);
    assert!(sim.view().score > 100);
}

#[test]
fn ineligible_head_is_skipped_and_waits() {
    let mut sim = Sim::new(scenario(), 0, Balance::mvp());
    add(&mut sim, &[0, 1]);
    place(&mut sim, 0, Shape::Square, 1);
    place(&mut sim, 0, Shape::Triangle, 1);
    let got = log(&mut sim, 60, |_, _| {});
    use Ev::*;
    assert_eq!(got, vec![B(7, 0, 1, 0), D(24, 0, 1, 1)]);
    assert_eq!(sim.view().queued, 1);
}

#[test]
fn passenger_boards_only_a_bus_in_its_board_mask() {
    let mut sim = Sim::new(scenario(), 0, Balance::mvp());
    add(&mut sim, &[1, 0]);
    add(&mut sim, &[1, 2]);
    place(&mut sim, 1, Shape::Square, 1);
    let got = log(&mut sim, 40, |_, _| {});
    use Ev::*;
    assert_eq!(got, vec![B(7, 1, 0, 1), D(24, 1, 0, 2)]);
}

#[test]
fn rider_stays_aboard_through_a_station_of_another_shape() {
    let mut sim = Sim::new(scenario(), 0, Balance::mvp());
    add(&mut sim, &[0, 1, 2]);
    place(&mut sim, 0, Shape::Square, 1);
    let got = log(&mut sim, 80, |_, _| {});
    assert!(!got.iter().any(|e| matches!(e, Ev::A(..))), "{got:?}");
    assert!(matches!(got.last(), Some(Ev::D(_, 0, 0, 2))), "{got:?}");
    assert_eq!(sim.view().score, 1);
}
