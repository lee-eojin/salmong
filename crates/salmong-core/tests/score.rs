use salmong_core::balance::Balance;
use salmong_core::ids::{StationId, Tick};
use salmong_core::replay::hash_sequence;
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::score::{FinalScore, merge};
use salmong_core::sim::{Command, Sim, SimEvent, SimMode};

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

fn fast() -> Balance {
    Balance {
        spawn_unit: 1,
        station_capacity: 0,
        overflow_limit: 3,
        ..Balance::mvp()
    }
}

fn run(score: u64, ticks: u64) -> FinalScore {
    FinalScore { score, ticks }
}

fn step_until_over(sim: &mut Sim) -> Vec<SimEvent> {
    let mut events = Vec::new();
    while sim.view().mode == SimMode::Running {
        sim.step(&mut events);
    }
    events
}

#[test]
fn no_final_score_while_running() {
    let mut sim = Sim::new(scenario(0), 1, Balance::mvp());
    assert_eq!(sim.final_score(), None);
    let mut events = Vec::new();
    sim.step(&mut events);
    assert_eq!(sim.final_score(), None);
}

#[test]
fn final_score_is_the_view_at_game_over() {
    let mut sim = Sim::new(scenario(1), 1, fast());
    let events = step_until_over(&mut sim);
    assert_eq!(events.last(), Some(&SimEvent::GameOver));
    let view = sim.view();
    assert_eq!(view.tick, Tick(3));
    assert_eq!(
        sim.final_score(),
        Some(FinalScore {
            score: view.score,
            ticks: 3
        })
    );
}

#[test]
fn final_score_counts_deliveries_and_stops_at_game_over() {
    let balance = Balance {
        travel_ticks: 1,
        service_ticks: 1,
        station_capacity: 2,
        overflow_limit: 40,
        spawn_unit: 30,
        ..Balance::mvp()
    };
    let mut sim = Sim::new(scenario(5), 1, balance);
    let mut events = Vec::new();
    sim.queue(Command::AddRoute {
        stops: vec![StationId(0), StationId(1)],
    });
    while sim.view().mode == SimMode::Running {
        sim.step(&mut events);
    }
    let delivered = events
        .iter()
        .filter(|e| matches!(e, SimEvent::Delivered { .. }))
        .count() as u64;
    let done = sim.final_score().expect("over");
    assert_eq!(done, run(24, 45));
    assert_eq!(done.score, delivered);
    assert_eq!(done.ticks, sim.view().tick.0);
    sim.step(&mut events);
    assert_eq!(sim.final_score(), Some(done));
}

#[test]
fn final_score_leaves_state_hash_alone() {
    let mut sim = Sim::new(scenario(1), 1, fast());
    step_until_over(&mut sim);
    let before = sim.state_hash();
    let _ = sim.final_score();
    assert_eq!(sim.state_hash(), before);
    let same = hash_sequence(scenario(1), 1, fast(), &[], 3);
    assert_eq!(same.last(), Some(&before));
}

#[test]
fn score_outranks_survival_and_survival_breaks_ties() {
    assert!(run(11, 1) > run(10, 999));
    assert!(run(10, 1000) > run(10, 999));
    assert_eq!(run(10, 999), run(10, 999));
    assert!(Some(run(0, 0)) > None);
}

#[test]
fn encoding_is_exact() {
    assert_eq!(run(0, 0).encode(), "salmong.best.1 0 0");
    assert_eq!(run(12, 3456).encode(), "salmong.best.1 12 3456");
    assert_eq!(
        run(u64::MAX, u64::MAX).encode(),
        "salmong.best.1 18446744073709551615 18446744073709551615"
    );
}

#[test]
fn decode_inverts_encode() {
    for r in [run(0, 0), run(12, 3456), run(u64::MAX, u64::MAX)] {
        assert_eq!(FinalScore::decode(&r.encode()), Some(r));
    }
}

#[test]
fn decode_refuses_non_canonical_text() {
    for bad in [
        "",
        "salmong.best.1",
        "salmong.best.1 ",
        "salmong.best.1 12",
        "salmong.best.1 12 ",
        "salmong.best.1  12 3456",
        "salmong.best.1 12  3456",
        "salmong.best.1 12 3456 ",
        "salmong.best.1 12 3456 7",
        "salmong.best.1 012 3456",
        "salmong.best.1 12 03456",
        "salmong.best.1 +12 3456",
        "salmong.best.1 12 +3456",
        "salmong.best.1 -1 3456",
        "salmong.best.1 1.5 3456",
        "salmong.best.1 x 3456",
        "salmong.best.1 12 x",
        "salmong.best.1 18446744073709551616 0",
        "salmong.best.1 0 18446744073709551616",
        "salmong.best.2 12 3456",
        "SALMONG.BEST.1 12 3456",
        " salmong.best.1 12 3456",
        "salmong.best.1 12 3456\n",
        "12 3456",
    ] {
        assert_eq!(FinalScore::decode(bad), None, "{bad:?}");
    }
}

#[test]
fn merge_keeps_the_better_run() {
    let stored = run(10, 500).encode();
    let s = Some(stored.as_str());
    assert_eq!(merge(s, run(11, 1)), run(11, 1));
    assert_eq!(merge(s, run(10, 501)), run(10, 501));
    assert_eq!(merge(s, run(10, 500)), run(10, 500));
    assert_eq!(merge(s, run(10, 499)), run(10, 500));
    assert_eq!(merge(s, run(9, 9999)), run(10, 500));
}

#[test]
fn merge_replaces_a_missing_or_corrupt_record() {
    assert_eq!(merge(None, run(0, 0)), run(0, 0));
    assert_eq!(merge(Some("garbage"), run(0, 1)), run(0, 1));
    assert_eq!(merge(Some("salmong.best.2 999 999"), run(1, 1)), run(1, 1));
}
