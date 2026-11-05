use salmong_core::balance::Balance;
use salmong_core::ids::{BusId, Dir, PassengerId, StationId, Tick};
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{Command, Sim};
use salmong_core::trace::{Action, BusState, Phase, TraceState};

fn hand() -> Sim {
    use Shape::*;
    let stations = [
        (0, 0, Circle, 1),
        (10, 0, Triangle, 0),
        (20, 0, Circle, 0),
        (30, 0, Square, 0),
    ]
    .into_iter()
    .map(|(x, y, shape, weight)| StationSpec {
        x,
        y,
        shape,
        at_tick: 0,
        weight,
    })
    .collect();
    let scenario = Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![
            RampPoint { tick: 0, rate: 0 },
            RampPoint { tick: 1, rate: 1 },
        ],
        stations,
    };
    let balance = Balance {
        travel_ticks: 1,
        start_buses: 2,
        capacity: 2,
        service_ticks: 1,
        spawn_unit: 1,
        max_alive: 1,
        station_capacity: 1000,
        overflow_limit: 1_000_000,
    };
    let mut sim = Sim::new(scenario, 1, balance);
    let st = |v: &[u16]| v.iter().map(|&i| StationId(i)).collect();
    sim.queue(Command::AddRoute {
        stops: st(&[0, 1, 2]),
    });
    sim.queue(Command::AddRoute { stops: st(&[2, 3]) });
    sim.step(&mut Vec::new());
    sim
}

fn bus(at: u16, dir: Dir, phase: Phase, riders: &[u32]) -> BusState {
    BusState {
        at: StationId(at),
        dir,
        phase,
        retiring: false,
        riders: riders.iter().map(|&p| PassengerId(p)).collect(),
    }
}

#[test]
fn hand_trace_matches_table() {
    use Dir::*;
    use Phase::*;
    let mut sim = hand();
    sim.start_trace();
    for _ in 0..4 {
        sim.step(&mut Vec::new());
    }
    let steps = sim.take_trace();
    let b = |i| BusId(i);
    let p0 = PassengerId(0);
    let expected = [
        (
            1,
            Action::Spawn {
                passenger: p0,
                want: Shape::Square,
            },
        ),
        (1, Action::Move { bus: b(0) }),
        (1, Action::Move { bus: b(1) }),
        (1, Action::EndAlight { bus: b(0) }),
        (1, Action::EndAlight { bus: b(1) }),
        (1, Action::Depart { bus: b(0) }),
        (1, Action::Depart { bus: b(1) }),
        (2, Action::Move { bus: b(0) }),
        (2, Action::Move { bus: b(1) }),
        (2, Action::EndAlight { bus: b(0) }),
        (2, Action::EndAlight { bus: b(1) }),
        (2, Action::Depart { bus: b(0) }),
        (2, Action::Depart { bus: b(1) }),
        (3, Action::Move { bus: b(0) }),
        (3, Action::Move { bus: b(1) }),
        (3, Action::EndAlight { bus: b(0) }),
        (3, Action::EndAlight { bus: b(1) }),
        (3, Action::Depart { bus: b(0) }),
        (3, Action::Depart { bus: b(1) }),
        (4, Action::Move { bus: b(0) }),
        (4, Action::Move { bus: b(1) }),
        (4, Action::EndAlight { bus: b(0) }),
        (4, Action::EndAlight { bus: b(1) }),
        (
            4,
            Action::Board {
                bus: b(0),
                passenger: p0,
            },
        ),
        (4, Action::Depart { bus: b(0) }),
        (4, Action::Depart { bus: b(1) }),
    ];
    let got = steps
        .iter()
        .map(|s| (s.tick.0, s.action))
        .collect::<Vec<_>>();
    assert_eq!(got, expected);
    for w in steps.windows(2) {
        assert_eq!(w[0].after, w[1].before);
    }
    let network = vec![
        vec![StationId(0), StationId(1), StationId(2)],
        vec![StationId(2), StationId(3)],
    ];
    let state = |buses, queue0: &[u32], wants: &[Shape]| TraceState {
        network: network.clone(),
        buses,
        queues: vec![
            queue0.iter().map(|&p| PassengerId(p)).collect(),
            vec![],
            vec![],
            vec![],
        ],
        delivered: vec![],
        edits: 0,
        wants: wants.to_vec(),
    };
    let sq = [Shape::Square];
    assert_eq!(
        steps[0].before,
        state(
            vec![bus(1, Fwd, Moving, &[]), bus(3, Fwd, Moving, &[])],
            &[],
            &[]
        )
    );
    assert_eq!(
        steps[0].after,
        state(
            vec![bus(1, Fwd, Moving, &[]), bus(3, Fwd, Moving, &[])],
            &[0],
            &sq
        )
    );
    assert_eq!(steps[1].after.buses[0], bus(1, Fwd, Alighting, &[]));
    assert_eq!(steps[3].after.buses[0], bus(1, Fwd, Boarding, &[]));
    assert_eq!(steps[4].after.buses[1], bus(3, Back, Boarding, &[]));
    assert_eq!(
        steps[6].after.buses,
        vec![bus(2, Fwd, Moving, &[]), bus(2, Back, Moving, &[])]
    );
    assert_eq!(
        steps[10].after.buses,
        vec![bus(2, Back, Boarding, &[]), bus(2, Fwd, Boarding, &[])]
    );
    assert_eq!(
        steps[12].after.buses,
        vec![bus(1, Back, Moving, &[]), bus(3, Fwd, Moving, &[])]
    );
    assert_eq!(
        steps[16].after.buses,
        vec![bus(1, Back, Boarding, &[]), bus(3, Back, Boarding, &[])]
    );
    assert_eq!(
        steps[18].after.buses,
        vec![bus(0, Back, Moving, &[]), bus(2, Back, Moving, &[])]
    );
    assert_eq!(
        steps[22].after.buses,
        vec![bus(0, Fwd, Boarding, &[]), bus(2, Fwd, Boarding, &[])]
    );
    assert_eq!(
        steps[23].after,
        state(
            vec![bus(0, Fwd, Boarding, &[0]), bus(2, Fwd, Boarding, &[])],
            &[],
            &sq
        )
    );
    assert_eq!(
        steps[25].after,
        state(
            vec![bus(1, Fwd, Moving, &[0]), bus(3, Fwd, Moving, &[])],
            &[],
            &sq
        )
    );
    assert_eq!(steps[25].tick, Tick(4));
}

#[test]
fn untraced_sim_records_nothing() {
    let mut sim = hand();
    sim.step(&mut Vec::new());
    assert!(sim.take_trace().is_empty());
}

#[test]
fn trace_started_late_knows_queued_wants() {
    let mut sim = hand();
    sim.step(&mut Vec::new());
    sim.step(&mut Vec::new());
    sim.start_trace();
    sim.step(&mut Vec::new());
    let steps = sim.take_trace();
    assert_eq!(steps[0].before.wants, vec![Shape::Square]);
    assert_eq!(steps[0].before.queues[0], vec![PassengerId(0)]);
    assert_eq!(steps[0].before.edits, 0);
}
