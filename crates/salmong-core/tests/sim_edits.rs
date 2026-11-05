use salmong_core::balance::Balance;
use salmong_core::ids::{BusId, Dir, PassengerId, RouteId, StationId};
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{BusMode, Command, Sim, SimEvent};

fn scenario() -> Scenario {
    use Shape::*;
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate: 0 }],
        stations: [
            (5, 5, Circle),
            (20, 20, Triangle),
            (40, 20, Square),
            (20, 30, Circle),
            (5, 35, Circle),
            (35, 35, Circle),
            (60, 35, Circle),
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

fn ids(route: &[u16]) -> Vec<StationId> {
    route.iter().map(|&i| StationId(i)).collect()
}

fn add(route: &[u16]) -> Command {
    Command::AddRoute { stops: ids(route) }
}

fn edit(route: u8, stops: &[u16]) -> Command {
    Command::EditRoute {
        route: RouteId(route),
        stops: ids(stops),
    }
}

fn remove(route: u8) -> Command {
    Command::RemoveRoute {
        route: RouteId(route),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Ev {
    B(u64, u16, u32, u16),
    A(u64, u16, u32, u16),
    D(u64, u16, u32, u16),
}
use Ev::*;

struct Run {
    sim: Sim,
    events: Vec<Ev>,
    arrivals: Vec<(u16, u64)>,
}

fn run(ticks: u64, mut before: impl FnMut(&mut Sim, u64), mut after: impl FnMut(&Sim, u64)) -> Run {
    let mut sim = Sim::new(scenario(), 0, Balance::mvp());
    let mut events = Vec::new();
    let mut arrivals = Vec::new();
    for t in 0..ticks {
        before(&mut sim, t);
        let mut raw = Vec::new();
        sim.step(&mut raw);
        for e in raw {
            events.push(match e {
                SimEvent::Boarded {
                    bus: BusId(b),
                    passenger: PassengerId(p),
                    station: StationId(s),
                } => B(t, b, p, s),
                SimEvent::Alighted {
                    bus: BusId(b),
                    passenger: PassengerId(p),
                    station: StationId(s),
                } => A(t, b, p, s),
                SimEvent::Delivered {
                    bus: BusId(b),
                    passenger: PassengerId(p),
                    station: StationId(s),
                } => D(t, b, p, s),
                SimEvent::CommandRejected => panic!("unexpected reject at {t}"),
                SimEvent::Spawned { .. } => panic!("unexpected spawn at {t}"),
            });
        }
        let view = sim.view();
        if let Some(bus) = view.buses.first() {
            if let Some((st, at)) = bus.last_arrival {
                if at.0 == t {
                    arrivals.push((st.0, t));
                }
            }
        }
        after(&sim, t);
    }
    Run {
        sim,
        events,
        arrivals,
    }
}

fn place(sim: &mut Sim, st: u16, want: Shape) {
    sim.place_passenger(StationId(st), want);
}

fn dir0(sim: &Sim) -> Dir {
    match sim.view().buses[0].mode {
        BusMode::Active { dir, .. } => dir,
        BusMode::Retiring => panic!("retiring"),
    }
}

#[test]
fn moving_bus_finishes_segment_then_reads_new_list() {
    let r = run(
        40,
        |s, t| match t {
            0 => {
                s.queue(add(&[0, 1]));
                place(s, 1, Shape::Square);
            }
            3 => s.queue(edit(0, &[2, 1])),
            _ => {}
        },
        |_, _| {},
    );
    assert_eq!(r.arrivals[..2], [(1, 10), (2, 27)]);
    assert_eq!(r.events, vec![B(17, 0, 0, 1), D(34, 0, 0, 2)]);
}

#[test]
fn edit_while_alighting_restarts_timer() {
    let r = run(
        56,
        |s, t| match t {
            0 => {
                s.queue(add(&[0, 1]));
                place(s, 0, Shape::Triangle);
                place(s, 0, Shape::Triangle);
            }
            30 => s.queue(edit(0, &[1, 2])),
            _ => {}
        },
        |_, _| {},
    );
    assert_eq!(
        r.events,
        vec![
            B(7, 0, 0, 0),
            B(15, 0, 1, 0),
            D(37, 0, 0, 1),
            D(45, 0, 1, 1)
        ]
    );
    assert_eq!(r.arrivals, [(1, 25), (2, 55)]);
}

#[test]
fn unedited_boarding_bus_drops_riders_whose_stay_turned_false() {
    let r = run(
        60,
        |s, t| match t {
            0 => {
                s.queue(add(&[0, 1, 3, 4, 5, 6, 2]));
                s.queue(add(&[3, 4]));
                place(s, 0, Shape::Square);
                place(s, 1, Shape::Circle);
            }
            20 => s.queue(edit(1, &[1, 2])),
            _ => {}
        },
        |_, _| {},
    );
    assert_eq!(
        r.events,
        vec![
            B(7, 0, 0, 0),
            A(27, 0, 0, 1),
            B(34, 0, 1, 1),
            B(37, 1, 0, 1),
            D(51, 0, 1, 3),
            D(54, 1, 0, 2),
        ]
    );
    assert_eq!(r.sim.view().score, 2);
}

#[test]
fn all_original_stops_removed() {
    let r = run(
        64,
        |s, t| match t {
            0 => {
                s.queue(add(&[0, 1, 2]));
                place(s, 0, Shape::Triangle);
                place(s, 0, Shape::Square);
            }
            28 => s.queue(edit(0, &[3, 5])),
            _ => {}
        },
        |_, _| {},
    );
    assert_eq!(
        r.events,
        vec![
            B(7, 0, 0, 0),
            B(15, 0, 1, 0),
            D(35, 0, 0, 1),
            A(43, 0, 1, 1)
        ]
    );
    assert_eq!(r.arrivals, [(1, 25), (3, 53), (5, 63)]);
}

#[test]
fn reversed_order_keeps_dir_and_reads_new_list() {
    let r = run(
        41,
        |s, t| match t {
            0 => s.queue(add(&[0, 1, 3])),
            5 => s.queue(edit(0, &[3, 1, 0])),
            _ => {}
        },
        |_, _| {},
    );
    assert_eq!(r.arrivals, [(1, 10), (0, 20), (1, 30), (3, 40)]);
}

#[test]
fn relocation_to_last_index_heads_back() {
    let mut dir = None;
    let r = run(
        31,
        |s, t| match t {
            0 => s.queue(add(&[0, 1])),
            5 => s.queue(edit(0, &[0, 2])),
            _ => {}
        },
        |s, t| {
            if t == 15 {
                dir = Some(dir0(s));
            }
        },
    );
    assert_eq!(r.arrivals, [(1, 10), (2, 20), (0, 30)]);
    assert_eq!(dir, Some(Dir::Back));
}

#[test]
fn relocation_tie_takes_lowest_index() {
    let mut dir = None;
    let r = run(
        21,
        |s, t| match t {
            0 => s.queue(add(&[0, 1])),
            5 => s.queue(edit(0, &[5, 0])),
            _ => {}
        },
        |s, t| {
            if t == 15 {
                dir = Some(dir0(s));
            }
        },
    );
    assert_eq!(r.arrivals, [(1, 10), (5, 20)]);
    assert_eq!(dir, Some(Dir::Fwd));
}

#[test]
fn repeated_edits_while_retiring() {
    let mut after40 = None;
    let r = run(
        60,
        |s, t| match t {
            0 => {
                s.queue(add(&[0, 1]));
                place(s, 0, Shape::Triangle);
                place(s, 0, Shape::Triangle);
                place(s, 1, Shape::Square);
            }
            20 => s.queue(remove(0)),
            22 => s.queue(add(&[0, 1])),
            30 => s.queue(edit(0, &[1, 2])),
            _ => {}
        },
        |s, t| {
            if t == 40 {
                let v = s.view();
                after40 = Some((
                    v.buses.iter().map(|b| b.id.0).collect::<Vec<_>>(),
                    v.free_buses,
                ));
            }
        },
    );
    assert_eq!(
        r.events,
        vec![
            B(7, 0, 0, 0),
            B(15, 0, 1, 0),
            D(32, 0, 0, 1),
            B(39, 1, 2, 1),
            D(40, 0, 1, 1),
            D(56, 1, 2, 2),
        ]
    );
    assert_eq!(after40, Some((vec![1], 3)));
}

#[test]
fn remove_while_boarding_parks_at_once() {
    let mut after3 = None;
    let r = run(
        30,
        |s, t| match t {
            0 => {
                s.queue(add(&[0, 1]));
                place(s, 0, Shape::Triangle);
            }
            3 => s.queue(remove(0)),
            _ => {}
        },
        |s, t| {
            if t == 3 {
                let v = s.view();
                after3 = Some((v.buses.len(), v.free_buses, v.route_count));
            }
        },
    );
    assert_eq!(r.events, vec![]);
    assert_eq!(after3, Some((0, 4, 0)));
}

#[test]
fn invalid_edits_leave_state_hash_unchanged() {
    let mk = || {
        let mut s = Sim::new(scenario(), 0, Balance::mvp());
        s.queue(add(&[0, 1, 3]));
        s.queue(add(&[2, 3]));
        s.queue(remove(1));
        place(&mut s, 0, Shape::Triangle);
        s
    };
    let (mut a, mut b) = (mk(), mk());
    let mut rejected = 0;
    for t in 0..40 {
        if t == 12 {
            a.queue(edit(5, &[0, 1]));
            a.queue(edit(0, &[0, 1, 0]));
            a.queue(edit(0, &[1]));
            a.queue(edit(0, &[0, 9]));
            a.queue(remove(3));
            a.queue(edit(1, &[2, 3]));
        }
        let (mut ea, mut eb) = (Vec::new(), Vec::new());
        a.step(&mut ea);
        b.step(&mut eb);
        rejected += ea
            .iter()
            .filter(|e| **e == SimEvent::CommandRejected)
            .count();
        assert_eq!(a.state_hash(), b.state_hash(), "tick {t}");
    }
    assert_eq!(rejected, 6);
    a.queue(edit(0, &[0, 1, 3]));
    let mut ea = Vec::new();
    a.step(&mut ea);
    b.step(&mut Vec::new());
    assert!(ea.is_empty());
    assert_ne!(a.state_hash(), b.state_hash());
}
