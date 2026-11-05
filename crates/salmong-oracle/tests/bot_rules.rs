use salmong_core::balance::Balance;
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::Sim;
use salmong_oracle::bots::{Bot, GameResult, Greedy};

fn quiet_sim() -> Sim {
    let stations = [
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
    .collect();
    let scenario = Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate: 0 }],
        stations,
    };
    Sim::new(scenario, 0, Balance::mvp())
}

#[test]
fn greedy_waits_while_no_passenger_is_queued() {
    let sim = quiet_sim();
    assert!(sim.view().free_buses > 0);
    assert!(Greedy.act(&sim.view()).is_empty());
}

#[test]
fn delivered_ratio_scales_both_sides() {
    let result = GameResult {
        spawned: 10,
        delivered: 6,
        survived: 0,
        over: false,
        commands: 0,
        alive: 0,
    };
    assert!(!result.delivered_ratio_at_least(2, 3));
    assert!(result.delivered_ratio_at_least(3, 5));
}
