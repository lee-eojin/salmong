use salmong_core::balance::Balance;
use salmong_core::ids::{RouteId, StationId, Tick};
use salmong_core::input::{MILLI, Pointer, PointerEvent, World};
use salmong_core::replay::{RULES_VERSION, ReplayError, ReplayHeader, hash_sequence};
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::score::FinalScore;
use salmong_core::sim::{BusPhase, BusView, Command, Sim, SimEvent, SimMode};
use salmong_core::trace::Action;

use Shape::{Circle as C, Square as S, Star as R, Triangle as T};

type Late = (u16, u16, Shape, u32, u32);

const SPREAD: [Late; 4] = [
    (50, 30, C, 5, 3),
    (20, 30, T, 9, 3),
    (5, 30, S, 9, 3),
    (60, 5, C, 20, 3),
];

fn scenario(ramp: &[(u32, u32)], start_weight: u32, late: &[Late]) -> Scenario {
    let start = [
        (12, 7, C, 0, start_weight),
        (30, 22, T, 0, start_weight),
        (48, 12, S, 0, start_weight),
    ];
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: ramp
            .iter()
            .map(|&(tick, rate)| RampPoint { tick, rate })
            .collect(),
        stations: start
            .iter()
            .chain(late)
            .map(|&(x, y, shape, at_tick, weight)| StationSpec {
                x,
                y,
                shape,
                at_tick,
                weight,
            })
            .collect(),
    }
}

fn quiet(late: &[Late]) -> Sim {
    Sim::new(scenario(&[(0, 0)], 3, late), 1, Balance::mvp())
}

fn ids(stops: &[u16]) -> Vec<StationId> {
    stops.iter().map(|&s| StationId(s)).collect()
}

fn step(sim: &mut Sim) -> Vec<SimEvent> {
    let mut events = Vec::new();
    sim.step(&mut events);
    events
}

fn steps(sim: &mut Sim, ticks: u64) -> Vec<(u64, SimEvent)> {
    let mut out = Vec::new();
    for _ in 0..ticks {
        let tick = sim.view().tick.0;
        out.extend(step(sim).into_iter().map(|e| (tick, e)));
    }
    out
}

fn joins(log: &[(u64, SimEvent)]) -> Vec<(u64, u16)> {
    log.iter()
        .filter_map(|(t, e)| match e {
            SimEvent::StationSpawned { station } => Some((*t, station.0)),
            _ => None,
        })
        .collect()
}

fn spawns(log: &[(u64, SimEvent)]) -> Vec<(u64, u16, Shape)> {
    log.iter()
        .filter_map(|(t, e)| match e {
            SimEvent::Spawned { station, want, .. } => Some((*t, station.0, *want)),
            _ => None,
        })
        .collect()
}

#[test]
fn each_station_joins_on_exactly_its_at_tick() {
    let mut sim = quiet(&SPREAD);
    let mut seen = Vec::new();
    for t in 0..30u64 {
        assert_eq!(sim.view().tick, Tick(t));
        seen.extend(joins(&steps(&mut sim, 1)));
        let live = 3 + SPREAD.iter().filter(|s| u64::from(s.3) <= t).count();
        let view = sim.view();
        assert_eq!(sim.snapshot().shapes().len(), live, "tick {t}");
        assert_eq!(view.queues.len(), live, "tick {t}");
        assert_eq!(view.overflow, vec![0; live], "tick {t}");
    }
    assert_eq!(seen, [(5, 3), (9, 4), (9, 5), (20, 6)]);
}

#[test]
fn same_tick_stations_join_in_scenario_order() {
    let mut sim = quiet(&SPREAD);
    steps(&mut sim, 9);
    assert_eq!(sim.snapshot().shapes(), [C, T, S, C]);
    let events = step(&mut sim);
    assert_eq!(
        events,
        [
            SimEvent::StationSpawned {
                station: StationId(4)
            },
            SimEvent::StationSpawned {
                station: StationId(5)
            },
        ]
    );
    assert_eq!(sim.snapshot().shapes(), [C, T, S, C, T, S]);
}

#[test]
fn the_table_recomputes_on_the_join_tick() {
    let mut sim = quiet(&SPREAD);
    sim.queue(Command::AddRoute {
        stops: ids(&[0, 1]),
    });
    step(&mut sim);
    assert_eq!(sim.view().table_rev, 1);
    for t in 1..30u64 {
        let rev = sim.view().table_rev;
        let due = SPREAD.iter().filter(|s| u64::from(s.3) == t).count() as u64;
        let mut events = Vec::new();
        sim.step_observed(&mut events, |sim, events| {
            if events
                .iter()
                .any(|e| matches!(e, SimEvent::StationSpawned { .. }))
            {
                assert_eq!(sim.view().table_rev, rev + due, "tick {t}");
            }
        });
        assert_eq!(sim.view().table_rev, rev + due, "tick {t}");
        assert_eq!(sim.view().table_rev, sim.snapshot().rev(), "tick {t}");
    }
}

#[test]
fn a_station_spawns_from_its_join_tick_with_an_empty_accumulator() {
    let late = [(50, 30, C, 5, 1), (20, 30, T, 7, 2)];
    let mut sim = Sim::new(scenario(&[(0, 15_000)], 0, &late), 1, Balance::mvp());
    let got = spawns(&steps(&mut sim, 12))
        .into_iter()
        .map(|(t, st, _)| (t, st))
        .collect::<Vec<_>>();
    assert_eq!(
        got,
        [
            (6, 3),
            (7, 4),
            (8, 3),
            (8, 4),
            (9, 4),
            (10, 3),
            (10, 4),
            (11, 4)
        ]
    );
}

#[test]
fn a_new_shape_is_wanted_from_its_join_tick() {
    let ramp = [(0, 0), (8399, 0), (8400, 30_000)];
    let balance = Balance {
        overflow_limit: u32::MAX,
        ..Balance::mvp()
    };
    let at = |tick| [(40, 32, R, tick, 1)];
    let on_time = scenario(&ramp, 40, &at(8400));
    let later = scenario(&ramp, 40, &at(8401));
    assert_eq!(on_time.validate(), Ok(()));
    assert_eq!(later.validate(), Ok(()));
    let first_tick = |scenario| {
        let mut sim = Sim::new(scenario, 1, balance);
        steps(&mut sim, 8400);
        spawns(&steps(&mut sim, 1))
    };
    let got = first_tick(on_time);
    assert_eq!(got.iter().filter(|s| s.1 == 3).count(), 1);
    assert!(got.iter().any(|s| s.2 == R));
    assert!(got.iter().all(|s| s.0 == 8400 && !(s.1 == 3 && s.2 == R)));
    let got = first_tick(later);
    assert_eq!(got.len(), 120);
    assert!(got.iter().all(|s| s.1 < 3 && s.2 != R));
}

#[test]
fn a_command_may_name_a_station_only_after_its_join_tick() {
    for (run_at, refused) in [(0, true), (4, true), (5, true), (6, false)] {
        let mut sim = quiet(&SPREAD);
        steps(&mut sim, run_at);
        sim.queue(Command::AddRoute {
            stops: ids(&[0, 3]),
        });
        let events = step(&mut sim);
        assert_eq!(
            events.contains(&SimEvent::CommandRejected),
            refused,
            "tick {run_at}"
        );
        assert_eq!(
            sim.view().route_count,
            usize::from(!refused),
            "tick {run_at}"
        );
    }
}

fn busy(late: &[Late]) -> Sim {
    let mut sim = quiet(late);
    for (station, want) in [(0, T), (0, S), (0, T), (1, C), (1, S), (2, C)] {
        sim.place_passenger(StationId(station), want);
    }
    sim.queue(Command::AddRoute {
        stops: ids(&[0, 1, 2]),
    });
    sim
}

fn bus_log(sim: &mut Sim, ticks: u64, edit_at: Option<u64>) -> Vec<Vec<BusView>> {
    let mut out = Vec::new();
    for t in 0..ticks {
        if edit_at == Some(t) {
            sim.queue(Command::EditRoute {
                route: RouteId(0),
                stops: ids(&[0, 1, 2]),
            });
        }
        step(sim);
        out.push(sim.view().buses);
    }
    out
}

const WINDOW: u64 = 90;

#[test]
fn a_join_leaves_every_bus_alone() {
    let base = bus_log(&mut busy(&[]), WINDOW, None);
    let phase = |log: &[Vec<BusView>], want: BusPhase| log.iter().any(|b| b[0].phase == want);
    assert!(phase(&base, BusPhase::Boarding));
    assert!(phase(&base, BusPhase::Alighting));
    for at in 1..WINDOW as u32 {
        let got = bus_log(&mut busy(&[(50, 30, C, at, 3)]), WINDOW, None);
        assert_eq!(got, base, "join at tick {at}");
    }
}

#[test]
fn an_edit_on_the_join_tick_still_restarts_stopped_buses() {
    let base = bus_log(&mut busy(&[]), WINDOW, None);
    let mut restarted = 0;
    for at in 1..WINDOW as u32 {
        let edit = Some(u64::from(at));
        let want = bus_log(&mut busy(&[]), WINDOW, edit);
        let got = bus_log(&mut busy(&[(50, 30, C, at, 3)]), WINDOW, edit);
        assert_eq!(got, want, "join and edit at tick {at}");
        restarted += usize::from(want != base);
    }
    assert!(restarted > 0);
}

#[test]
fn a_join_is_not_a_network_edit() {
    let mut sim = busy(&SPREAD);
    step(&mut sim);
    sim.start_trace();
    assert_eq!(joins(&steps(&mut sim, 30)).len(), 4);
    let trace = sim.take_trace();
    assert!(trace.iter().all(|s| s.action != Action::EditNetwork));
    let last = trace.last().expect("the bus moved");
    assert_eq!(last.after.edits, 0);
    assert_eq!(last.after.queues.len(), 7);
}

fn overflowing() -> Sim {
    let balance = Balance {
        station_capacity: 0,
        overflow_limit: 3,
        ..Balance::mvp()
    };
    let late = [(50, 30, C, 5, 1), (20, 30, T, 9, 1)];
    Sim::new(scenario(&[(0, 30_000)], 0, &late), 1, balance)
}

#[test]
fn a_joined_station_can_overflow_and_end_the_game() {
    let mut sim = overflowing();
    let log = steps(&mut sim, 8);
    let ends = log
        .iter()
        .filter(|(_, e)| !matches!(e, SimEvent::Spawned { .. }))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        ends,
        [
            (
                5,
                SimEvent::StationSpawned {
                    station: StationId(3)
                }
            ),
            (
                5,
                SimEvent::OverflowStarted {
                    station: StationId(3)
                }
            ),
            (7, SimEvent::GameOver),
        ]
    );
    assert_eq!(sim.view().overflow, [0, 0, 0, 3]);
    assert_eq!(sim.final_score(), Some(FinalScore { score: 0, ticks: 8 }));
}

#[test]
fn no_station_joins_after_game_over() {
    let mut sim = overflowing();
    assert!(joins(&steps(&mut sim, 30)) == [(5, 3)]);
    assert_eq!(sim.snapshot().shapes().len(), 4);
    assert_eq!(sim.view().tick, Tick(8));
}

#[test]
fn a_bus_carries_passengers_to_and_from_a_joined_station() {
    let mut sim = quiet(&[(40, 8, S, 5, 3)]);
    sim.queue(Command::AddRoute {
        stops: ids(&[0, 1]),
    });
    steps(&mut sim, 6);
    sim.queue(Command::EditRoute {
        route: RouteId(0),
        stops: ids(&[0, 1, 3]),
    });
    sim.place_passenger(StationId(3), C);
    sim.place_passenger(StationId(0), S);
    let log = steps(&mut sim, 200);
    let delivered = log
        .iter()
        .filter_map(|(_, e)| match e {
            SimEvent::Delivered { station, .. } => Some(station.0),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(delivered, [0, 3]);
    assert_eq!(sim.view().score, 2);
}

fn step_until(sim: &mut Sim, done: impl Fn(&BusPhase) -> bool) -> BusPhase {
    for _ in 0..200 {
        let phase = sim.view().buses[0].phase.clone();
        if done(&phase) {
            return phase;
        }
        step(sim);
    }
    panic!("the bus never reached that phase");
}

#[test]
fn a_bus_cut_off_at_a_joined_station_heads_for_the_nearest_stop() {
    let mut sim = quiet(&[(40, 8, S, 5, 3)]);
    sim.queue(Command::AddRoute {
        stops: ids(&[1, 2]),
    });
    steps(&mut sim, 6);
    sim.queue(Command::EditRoute {
        route: RouteId(0),
        stops: ids(&[1, 2, 3]),
    });
    step_until(&mut sim, |p| {
        matches!(
            p,
            BusPhase::Moving {
                to: StationId(3),
                ..
            }
        )
    });
    sim.queue(Command::EditRoute {
        route: RouteId(0),
        stops: ids(&[1, 2]),
    });
    let phase = step_until(&mut sim, |p| {
        matches!(
            p,
            BusPhase::Moving {
                from: StationId(3),
                ..
            }
        )
    });
    assert_eq!(
        phase,
        BusPhase::Moving {
            from: StationId(3),
            to: StationId(2),
            progress: 0
        }
    );
}

#[test]
fn a_pointer_reaches_a_station_only_after_it_joins() {
    let world = |x: i64, y: i64| {
        PointerEvent::Move(World {
            x: x * MILLI,
            y: y * MILLI,
        })
    };
    let grid = scenario(&[(0, 0)], 3, &SPREAD);
    let drag = |sim: &Sim| {
        let net = sim.snapshot();
        let mut p = Pointer::default();
        for ev in [world(12, 7), PointerEvent::Press, world(50, 30)] {
            p.apply(ev, &grid, &net);
        }
        p.apply(PointerEvent::Release, &grid, &net)
    };
    let mut sim = Sim::new(grid.clone(), 1, Balance::mvp());
    steps(&mut sim, 5);
    assert_eq!(drag(&sim), None);
    step(&mut sim);
    let cmd = drag(&sim).expect("the station has joined");
    assert_eq!(
        cmd,
        Command::AddRoute {
            stops: ids(&[0, 3])
        }
    );
    sim.queue(cmd);
    assert!(!step(&mut sim).contains(&SimEvent::CommandRejected));
}

#[test]
fn the_hash_splits_on_the_join_tick() {
    let without = hash_sequence(scenario(&[(0, 0)], 3, &[]), 1, Balance::mvp(), &[], 8);
    let with = hash_sequence(
        scenario(&[(0, 0)], 3, &[(50, 30, C, 5, 3)]),
        1,
        Balance::mvp(),
        &[],
        8,
    );
    assert_eq!(without[..6], with[..6]);
    assert!((6..=8).all(|i| without[i] != with[i]));
}

#[test]
fn hash_sequences_repeat_through_joins() {
    let body = [(
        Tick(6),
        Command::AddRoute {
            stops: ids(&[0, 3]),
        },
    )];
    let run = || {
        hash_sequence(
            scenario(&[(0, 40_000)], 3, &SPREAD),
            7,
            Balance::mvp(),
            &body,
            300,
        )
    };
    assert_eq!(run(), run());
}

#[test]
fn replays_from_before_station_spawning_are_refused() {
    assert_eq!(RULES_VERSION, 2);
    let header = |rules_version| ReplayHeader {
        rules_version,
        pack_sha256: [0; 32],
        scenario_id: 0,
        seed: 1,
        balance: Balance::mvp(),
    };
    let grid = scenario(&[(0, 0)], 3, &SPREAD);
    let check = |v: u32| header(v).matches(&[0; 32], &grid, 1, Balance::mvp());
    assert_eq!(check(2), Ok(()));
    assert_eq!(check(1), Err(ReplayError::RulesVersion));
}

#[test]
fn a_scenario_with_no_late_station_never_emits_a_join() {
    let mut sim = quiet(&[]);
    assert_eq!(joins(&steps(&mut sim, 1000)), []);
    assert_eq!(sim.view().mode, SimMode::Running);
}
