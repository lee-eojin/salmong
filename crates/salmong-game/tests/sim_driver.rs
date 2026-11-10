#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::WindowEvent;
use salmong_core::balance::Balance;
use salmong_core::ids::{RouteId, StationId, Tick};
use salmong_core::replay::hash_sequence;
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{Command, Sim, SimEvent, SimMode};
use salmong_game::input::{CommandLog, InputPlugin, SimRes};
use salmong_game::sim::{ReplayFeed, SimEventMessage, SimEvents, SimPlugin, step_sim};
use salmong_pack::parse_pack;

const PACK: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");
const N: usize = 600;

#[derive(Resource, Default)]
struct Hashes(Vec<u64>);

#[derive(Resource, Default)]
struct Seen(Vec<SimEvent>);

fn record(sim: Res<SimRes>, mut hashes: ResMut<Hashes>) {
    hashes.0.push(sim.sim.state_hash());
}

fn collect(mut reader: MessageReader<SimEventMessage>, mut seen: ResMut<Seen>) {
    seen.0.extend(reader.read().map(|m| m.0.clone()));
}

fn pack_scenario() -> Scenario {
    parse_pack(PACK).expect("pack").scenarios[0].clone()
}

fn small_scenario() -> Scenario {
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
            weight: 1,
        })
        .collect(),
    }
}

fn stops(s: &[u16]) -> Vec<StationId> {
    s.iter().map(|&i| StationId(i)).collect()
}

fn body() -> Vec<(Tick, Command)> {
    vec![
        (
            Tick(5),
            Command::AddRoute {
                stops: stops(&[0, 1]),
            },
        ),
        (
            Tick(40),
            Command::AddRoute {
                stops: stops(&[1, 2]),
            },
        ),
        (
            Tick(40),
            Command::EditRoute {
                route: RouteId(0),
                stops: stops(&[2, 0, 1]),
            },
        ),
        (Tick(200), Command::RemoveRoute { route: RouteId(1) }),
        (
            Tick(300),
            Command::EditRoute {
                route: RouteId(5),
                stops: stops(&[0, 2]),
            },
        ),
    ]
}

fn harness(scenario: Scenario, feed: Option<Vec<(Tick, Command)>>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<WindowEvent>()
        .add_plugins((InputPlugin, SimPlugin))
        .init_resource::<Hashes>()
        .init_resource::<Seen>()
        .add_systems(
            FixedUpdate,
            record.after(step_sim).run_if(resource_exists::<SimRes>),
        )
        .add_systems(Update, collect);
    let seed = u64::from(scenario.seed);
    app.insert_resource(SimRes {
        sim: Sim::new(scenario.clone(), seed, Balance::mvp()),
        scenario,
    });
    if let Some(b) = feed {
        app.insert_resource(ReplayFeed::new(b));
    }
    app
}

fn tick_of(app: &App) -> u64 {
    app.world().resource::<SimRes>().sim.view().tick.0
}

fn timestep(app: &App) -> Duration {
    app.world().resource::<Time<Fixed>>().timestep()
}

fn frame(app: &mut App, d: Duration) -> u64 {
    let before = tick_of(app);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(d));
    app.update();
    tick_of(app) - before
}

fn warm_up(app: &mut App) {
    assert_eq!(frame(app, Duration::ZERO), 0);
}

fn plain_events(scenario: &Scenario, body: &[(Tick, Command)], ticks: usize) -> Vec<SimEvent> {
    let mut sim = Sim::new(scenario.clone(), u64::from(scenario.seed), Balance::mvp());
    let mut out = Vec::new();
    let mut bi = 0;
    for t in 0..ticks as u64 {
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        sim.step(&mut out);
    }
    out
}

fn run_rate(frames: impl Iterator<Item = Duration>) -> Vec<u64> {
    let scenario = pack_scenario();
    let body = body();
    let expected = hash_sequence(
        scenario.clone(),
        u64::from(scenario.seed),
        Balance::mvp(),
        &body,
        N as u64 + 1,
    );
    let mut app = harness(scenario.clone(), Some(body.clone()));
    assert_eq!(
        app.world().resource::<SimRes>().sim.state_hash(),
        expected[0]
    );
    warm_up(&mut app);
    let mut counts = Vec::new();
    for d in frames {
        if app.world().resource::<Hashes>().0.len() >= N {
            break;
        }
        counts.push(frame(&mut app, d));
    }
    let hashes = app.world().resource::<Hashes>().0.clone();
    assert!(hashes.len() >= N, "only {} ticks", hashes.len());
    assert_eq!(hashes[..], expected[1..=hashes.len()]);
    assert_eq!(app.world().resource::<CommandLog>().0, body);
    assert_eq!(app.world().resource::<ReplayFeed>().remaining(), 0);
    let view = app.world().resource::<SimRes>().sim.view();
    assert_eq!(view.mode, SimMode::Running);
    assert!(view.route_count >= 1);
    let seen = app.world().resource::<Seen>().0.clone();
    assert!(seen.iter().any(|e| matches!(e, SimEvent::Boarded { .. })));
    assert!(seen.iter().any(|e| matches!(e, SimEvent::CommandRejected)));
    assert_eq!(seen, plain_events(&scenario, &body, hashes.len()));
    counts
}

fn fixed_step() -> Duration {
    Time::<Fixed>::from_hz(salmong_game::sim::TICK_HZ).timestep()
}

#[test]
fn hashes_match_headless_at_30hz() {
    let t = Duration::from_nanos(33_333_333);
    let counts = run_rate(std::iter::repeat(t));
    assert_eq!(counts.len(), N);
    assert!(counts.iter().all(|&c| c == 1), "{counts:?}");
}

#[test]
fn hashes_match_headless_at_60hz() {
    let t = Duration::from_nanos(33_333_333);
    let counts = run_rate(std::iter::repeat(t / 2));
    assert!(counts.iter().all(|&c| c <= 1));
    assert!(
        (2 * N..=2 * N + 2).contains(&counts.len()),
        "{}",
        counts.len()
    );
}

#[test]
fn hashes_match_headless_at_144hz() {
    let counts = run_rate(std::iter::repeat(Duration::from_secs_f64(1.0 / 144.0)));
    assert!(counts.iter().all(|&c| c <= 1));
    assert!(counts.len() > 4 * N, "{}", counts.len());
}

#[test]
fn hashes_match_headless_at_10hz_capped() {
    let t = Duration::from_nanos(33_333_333);
    let counts = run_rate(std::iter::repeat(t * 3));
    assert!(counts.iter().all(|&c| c == 2), "{counts:?}");
    assert_eq!(counts.len(), N / 2);
}

#[test]
fn hashes_match_headless_on_irregular_frames() {
    let ms = [1u64, 40, 7, 16, 70, 33, 2, 120, 16, 0, 250, 9];
    let counts = run_rate(ms.iter().cycle().map(|&m| Duration::from_millis(m)));
    assert!(counts.iter().all(|&c| c <= 2));
    for k in 0..=2 {
        assert!(counts.contains(&k), "no frame ran {k} ticks");
    }
}

fn assert_clocks(app: &App) {
    let t = app.world().resource::<Time<Fixed>>().timestep();
    assert_eq!(t, Duration::from_nanos(33_333_333));
    assert_eq!(app.world().resource::<Time<Virtual>>().max_delta(), t * 2);
}

#[test]
fn plugin_sets_30hz_and_two_tick_cap() {
    let mut after = App::new();
    after
        .add_plugins(MinimalPlugins)
        .add_plugins((InputPlugin, SimPlugin));
    assert_clocks(&after);
    let mut before = App::new();
    before
        .add_plugins((InputPlugin, SimPlugin))
        .add_plugins(MinimalPlugins);
    assert_clocks(&before);
    assert_eq!(fixed_step(), Duration::from_nanos(33_333_333));
}

#[test]
fn long_frame_runs_exactly_two_ticks() {
    let mut app = harness(small_scenario(), None);
    warm_up(&mut app);
    let t = timestep(&app);
    for d in [
        t * 2 + Duration::from_nanos(1),
        t * 3,
        t * 5,
        Duration::from_millis(250),
        Duration::from_secs(10),
    ] {
        assert_eq!(frame(&mut app, d), 2, "{d:?}");
    }
    let start = tick_of(&app);
    for _ in 0..10 {
        frame(&mut app, t * 5);
    }
    assert_eq!(tick_of(&app) - start, 20);
}

#[test]
fn carried_overstep_cannot_buy_a_third_tick() {
    let mut app = harness(small_scenario(), None);
    warm_up(&mut app);
    let t = timestep(&app);
    assert_eq!(frame(&mut app, t - Duration::from_nanos(1)), 0);
    assert_eq!(frame(&mut app, Duration::from_secs(10)), 2);
    assert_eq!(
        app.world().resource::<Time<Fixed>>().overstep(),
        t - Duration::from_nanos(1)
    );
}

#[test]
fn no_sim_res_runs_nothing() {
    let mut app = harness(small_scenario(), None);
    app.world_mut().remove_resource::<SimRes>();
    warm_up_without_sim(&mut app);
    let t = Duration::from_nanos(33_333_333);
    for _ in 0..5 {
        app.insert_resource(TimeUpdateStrategy::ManualDuration(t));
        app.update();
    }
    assert!(app.world().resource::<SimEvents>().0.is_empty());
    assert!(app.world().resource::<CommandLog>().0.is_empty());
    assert!(app.world().resource::<Seen>().0.is_empty());
    assert!(app.world().resource::<Hashes>().0.is_empty());
}

fn warm_up_without_sim(app: &mut App) {
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.update();
}
