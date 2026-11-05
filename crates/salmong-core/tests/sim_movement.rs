use salmong_core::balance::Balance;
use salmong_core::ids::{Dir, StationId, Tick};
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{BusMode, BusPhase, Command, Sim, SimEvent};

fn scenario() -> Scenario {
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate: 40 }],
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

fn stops(ids: &[u16]) -> Vec<StationId> {
    ids.iter().map(|&i| StationId(i)).collect()
}

fn run_arrivals(route: &[u16], steps: u64) -> (Vec<(u16, u64)>, Vec<(u64, Dir, u16)>) {
    let mut sim = Sim::new(scenario(), 0, Balance::mvp());
    sim.queue(Command::AddRoute {
        stops: stops(route),
    });
    let mut arrivals = Vec::new();
    let mut dir_at_arrival = Vec::new();
    for _ in 0..steps {
        let tick = sim.view().tick;
        let mut events = Vec::new();
        sim.step(&mut events);
        let view = sim.view();
        let bus = &view.buses[0];
        if let Some((st, at)) = bus.last_arrival {
            if at == tick {
                arrivals.push((st.0, at.0));
                dir_at_arrival.push((
                    at.0,
                    match bus.mode {
                        BusMode::Active { dir, .. } => dir,
                        BusMode::Retiring => panic!("retiring"),
                    },
                    match &bus.phase {
                        BusPhase::Moving { to, .. } => to.0,
                        _ => u16::MAX,
                    },
                ));
            }
        }
    }
    (arrivals, dir_at_arrival)
}

#[test]
fn three_stop_shuttle_arrivals_match_hand_table() {
    let (arrivals, dirs) = run_arrivals(&[0, 1, 2], 61);
    assert_eq!(
        arrivals,
        vec![(1, 10), (2, 20), (1, 30), (0, 40), (1, 50), (2, 60),]
    );
    let at20 = dirs.iter().find(|(t, _, _)| *t == 20).unwrap();
    assert_eq!(at20.1, Dir::Back);
    assert_eq!(at20.2, 1);
    let at40 = dirs.iter().find(|(t, _, _)| *t == 40).unwrap();
    assert_eq!(at40.1, Dir::Fwd);
    assert_eq!(at40.2, 1);
}

#[test]
fn two_stop_shuttle_arrivals_match_hand_table() {
    let (arrivals, _) = run_arrivals(&[0, 1], 41);
    assert_eq!(arrivals, vec![(1, 10), (0, 20), (1, 30), (0, 40)]);
}

#[test]
fn add_route_rejects_leave_the_sim_unchanged() {
    let mut sim = Sim::new(scenario(), 0, Balance::mvp());
    let before = sim.view();
    let mut events = Vec::new();
    sim.queue(Command::AddRoute { stops: stops(&[0]) });
    sim.step(&mut events);
    assert_eq!(events, vec![SimEvent::CommandRejected]);
    let after = sim.view();
    assert_eq!(after.route_count, before.route_count);
    assert_eq!(after.free_buses, before.free_buses);
    assert!(after.buses.is_empty());
    assert_eq!(after.tick, Tick(1));
}

#[test]
fn add_route_needs_a_free_bus() {
    let mut sim = Sim::new(scenario(), 0, Balance::mvp());
    for _ in 0..4 {
        sim.queue(Command::AddRoute {
            stops: stops(&[0, 1]),
        });
    }
    let mut events = Vec::new();
    sim.step(&mut events);
    assert!(events.is_empty());
    assert_eq!(sim.view().free_buses, 0);
    assert_eq!(sim.view().buses.len(), 4);
    sim.queue(Command::AddRoute {
        stops: stops(&[0, 2]),
    });
    sim.step(&mut events);
    assert_eq!(events, vec![SimEvent::CommandRejected]);
    assert_eq!(sim.view().buses.len(), 4);
    assert_eq!(sim.view().route_count, 4);
}
