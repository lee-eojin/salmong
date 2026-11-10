#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{AppLifecycle, WindowEvent, WindowOccluded};
use salmong_core::balance::Balance;
use salmong_core::ids::{RouteId, StationId, Tick};
use salmong_core::replay::hash_sequence;
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{Command, Sim, SimEvent, SimMode};
use salmong_game::clock::{Clock, ClockCommand, ClockPlugin, Speed};
use salmong_game::input::{CommandLog, InputPlugin, SimRes};
use salmong_game::sim::{ReplayFeed, SimEventMessage, SimPlugin, step_sim};
use salmong_pack::parse_pack;

const PACK: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");
const N: usize = 600;
const T: Duration = Duration::from_nanos(33_333_333);
const NS: Duration = Duration::from_nanos(1);

#[derive(Resource, Default)]
struct Hashes(Vec<u64>);

#[derive(Resource, Default)]
struct Seen(Vec<SimEvent>);

#[derive(Clone)]
enum Msg {
    Clock(ClockCommand),
    Window(WindowEvent),
}

#[derive(Clone, Copy, Debug)]
enum Expect {
    Exactly(u64),
    AtMost(u64),
}

fn record(sim: Res<SimRes>, mut hashes: ResMut<Hashes>) {
    hashes.0.push(sim.sim.state_hash());
}

fn collect(mut reader: MessageReader<SimEventMessage>, mut seen: ResMut<Seen>) {
    seen.0.extend(reader.read().map(|m| m.0.clone()));
}

fn pause() -> Msg {
    Msg::Clock(ClockCommand::Pause)
}

fn resume() -> Msg {
    Msg::Clock(ClockCommand::Resume)
}

fn speed(s: Speed) -> Msg {
    Msg::Clock(ClockCommand::SetSpeed(s))
}

fn occluded(occluded: bool) -> Msg {
    Msg::Window(WindowEvent::WindowOccluded(WindowOccluded {
        window: Entity::PLACEHOLDER,
        occluded,
    }))
}

fn life(l: AppLifecycle) -> Msg {
    Msg::Window(WindowEvent::AppLifecycle(l))
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
            Tick(0),
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
        .add_plugins((InputPlugin, SimPlugin, ClockPlugin))
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

fn hash_of(app: &App) -> u64 {
    app.world().resource::<SimRes>().sim.state_hash()
}

fn clock(app: &App) -> Clock {
    *app.world().resource::<Clock>()
}

fn virt(app: &App) -> &Time<Virtual> {
    app.world().resource::<Time<Virtual>>()
}

fn overstep(app: &App) -> Duration {
    app.world().resource::<Time<Fixed>>().overstep()
}

fn frame(app: &mut App, msgs: &[Msg], d: Duration) -> u64 {
    for m in msgs {
        match m {
            Msg::Clock(c) => {
                app.world_mut().write_message(*c);
            }
            Msg::Window(w) => {
                app.world_mut().write_message(w.clone());
            }
        }
    }
    let before = tick_of(app);
    let hash = hash_of(app);
    let recorded = app.world().resource::<Hashes>().0.len();
    app.insert_resource(TimeUpdateStrategy::ManualDuration(d));
    app.update();
    let ran = tick_of(app) - before;
    if ran == 0 {
        assert_eq!(hash_of(app), hash);
        assert_eq!(app.world().resource::<Hashes>().0.len(), recorded);
    }
    ran
}

fn warm_up(app: &mut App) {
    assert_eq!(frame(app, &[], Duration::ZERO), 0);
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

fn ms(m: u64) -> Duration {
    Duration::from_millis(m)
}

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

fn cycle() -> Vec<(&'static str, Vec<Msg>, Duration, Expect)> {
    use Expect::{AtMost, Exactly};
    let mut c = Vec::new();
    let mut step = |name: &'static str, first: Vec<Msg>, frames: Vec<(Duration, Expect)>| {
        for (i, (d, e)) in frames.into_iter().enumerate() {
            let msgs = if i == 0 { first.clone() } else { Vec::new() };
            c.push((name, msgs, d, e));
        }
    };
    step(
        "a pause",
        vec![pause()],
        vec![(T, Exactly(0)), (secs(10), Exactly(0))],
    );
    step("b resume", vec![resume()], vec![(T, Exactly(0))]);
    let mut stall = vec![(T, Exactly(1)); 8];
    stall.push((ms(250), Exactly(2)));
    step("c stall", vec![], stall);
    let mut x2 = vec![(T, Exactly(2)); 6];
    x2.push((secs(10), Exactly(2)));
    step("d x2", vec![speed(Speed::X2)], x2);
    step(
        "e hide",
        vec![occluded(true)],
        vec![
            (T, Exactly(0)),
            (secs(1), Exactly(0)),
            (secs(1), Exactly(0)),
        ],
    );
    step(
        "f show",
        vec![occluded(false)],
        vec![(secs(10), Exactly(0))],
    );
    step(
        "g x2 at 60 fps",
        vec![],
        vec![(Duration::from_nanos(16_666_667), AtMost(1)); 4],
    );
    step(
        "h x1 irregular",
        vec![speed(Speed::X1)],
        vec![
            (ms(1), AtMost(2)),
            (ms(40), AtMost(2)),
            (ms(7), AtMost(2)),
            (ms(70), AtMost(2)),
        ],
    );
    step(
        "i hide and show together",
        vec![occluded(true), occluded(false)],
        vec![(secs(10), Exactly(0))],
    );
    step("j steady", vec![], vec![(T, Exactly(1)); 3]);
    step(
        "k will suspend",
        vec![life(AppLifecycle::WillSuspend)],
        vec![(T, Exactly(0))],
    );
    step(
        "k suspended",
        vec![life(AppLifecycle::Suspended)],
        vec![(secs(1), Exactly(0))],
    );
    step(
        "k will resume",
        vec![life(AppLifecycle::WillResume)],
        vec![(secs(10), Exactly(0))],
    );
    step(
        "k running",
        vec![life(AppLifecycle::Running)],
        vec![(T, Exactly(1))],
    );
    step(
        "l pause x2",
        vec![pause(), speed(Speed::X2)],
        vec![(T, Exactly(0))],
    );
    step(
        "l resume x1",
        vec![resume(), speed(Speed::X1)],
        vec![(T, Exactly(0))],
    );
    step("l after", vec![], vec![(T, Exactly(1))]);
    c
}

#[test]
fn transitions_keep_replay_hashes() {
    let scenario = pack_scenario();
    let body = body();
    let expected = hash_sequence(
        scenario.clone(),
        u64::from(scenario.seed),
        Balance::mvp(),
        &body,
        N as u64 + 3,
    );
    let mut app = harness(scenario.clone(), Some(body.clone()));
    assert_eq!(hash_of(&app), expected[0]);
    warm_up(&mut app);
    let plan = cycle();
    let mut cycles = 0;
    'run: loop {
        for (name, msgs, d, e) in &plan {
            if app.world().resource::<Hashes>().0.len() >= N {
                break 'run;
            }
            let ran = frame(&mut app, msgs, *d);
            assert!(ran <= 2, "step {name}: {ran} ticks");
            match e {
                Expect::Exactly(k) => assert_eq!(ran, *k, "step {name}, cycle {cycles}"),
                Expect::AtMost(k) => assert!(ran <= *k, "step {name}, cycle {cycles}: {ran}"),
            }
        }
        cycles += 1;
    }
    assert!(cycles >= 2, "only {cycles} cycles");
    let hashes = app.world().resource::<Hashes>().0.clone();
    assert!(hashes.len() >= N);
    assert_eq!(app.world().resource::<CommandLog>().0, body);
    assert_eq!(app.world().resource::<ReplayFeed>().remaining(), 0);
    assert_eq!(
        app.world().resource::<SimRes>().sim.view().mode,
        SimMode::Running
    );
    let seen = app.world().resource::<Seen>().0.clone();
    assert_eq!(seen, plain_events(&scenario, &body, hashes.len()));
    assert_eq!(hashes[..], expected[1..=hashes.len()]);
}

fn small() -> App {
    let mut app = harness(small_scenario(), None);
    warm_up(&mut app);
    app
}

fn halted_frames_run_no_ticks(halt: Msg) -> App {
    let mut app = small();
    assert_eq!(frame(&mut app, &[], T), 1);
    let tick = tick_of(&app);
    let hash = hash_of(&app);
    assert_eq!(frame(&mut app, &[halt], T), 0);
    assert_eq!(frame(&mut app, &[], secs(10)), 0);
    assert_eq!(frame(&mut app, &[], T), 0);
    assert_eq!(tick_of(&app), tick);
    assert_eq!(hash_of(&app), hash);
    assert!(clock(&app).halted());
    assert!(virt(&app).is_paused());
    app
}

#[test]
fn paused_frames_run_no_ticks() {
    let app = halted_frames_run_no_ticks(pause());
    assert!(clock(&app).paused());
    assert!(!clock(&app).hidden());
}

#[test]
fn hidden_frames_run_no_ticks() {
    let app = halted_frames_run_no_ticks(occluded(true));
    assert!(clock(&app).hidden());
    assert!(!clock(&app).paused());
}

#[test]
fn suspended_frames_run_no_ticks() {
    let mut app = small();
    assert_eq!(frame(&mut app, &[], T), 1);
    assert_eq!(frame(&mut app, &[life(AppLifecycle::WillSuspend)], T), 0);
    assert_eq!(
        frame(&mut app, &[life(AppLifecycle::Suspended)], secs(10)),
        0
    );
    assert!(clock(&app).hidden());
    assert_eq!(
        frame(&mut app, &[life(AppLifecycle::WillResume)], secs(10)),
        0
    );
    assert!(!clock(&app).hidden());
    assert_eq!(frame(&mut app, &[], T), 1);
}

#[test]
fn resume_clears_overstep_without_a_burst() {
    let mut app = small();
    assert_eq!(frame(&mut app, &[], T - NS), 0);
    assert_eq!(frame(&mut app, &[pause()], T), 0);
    assert_eq!(frame(&mut app, &[resume()], secs(10)), 0);
    assert_eq!(overstep(&app), Duration::ZERO);
    assert!(!virt(&app).is_paused());
    assert_eq!(frame(&mut app, &[], T - NS), 0);
    assert_eq!(frame(&mut app, &[], T), 1);
}

#[test]
fn visible_clears_overstep_without_a_burst() {
    let mut app = small();
    assert_eq!(frame(&mut app, &[], T - NS), 0);
    assert_eq!(frame(&mut app, &[occluded(true)], T), 0);
    assert_eq!(frame(&mut app, &[occluded(false)], secs(10)), 0);
    assert_eq!(overstep(&app), Duration::ZERO);
    assert_eq!(frame(&mut app, &[], T - NS), 0);
    assert_eq!(frame(&mut app, &[], T), 1);
    assert_eq!(
        frame(&mut app, &[occluded(true), occluded(false)], secs(10)),
        0
    );
    assert_eq!(overstep(&app), Duration::ZERO);
    assert_eq!(frame(&mut app, &[], T), 1);
}

#[test]
fn x2_runs_two_ticks_and_keeps_the_cap() {
    let mut app = small();
    assert_eq!(frame(&mut app, &[speed(Speed::X2)], T), 2);
    assert_eq!(virt(&app).delta(), T * 2);
    assert_eq!(virt(&app).relative_speed(), 2.0);
    for d in [T * 2, ms(250), secs(10)] {
        assert_eq!(frame(&mut app, &[], d), 2, "{d:?}");
    }
    let start = tick_of(&app);
    for _ in 0..10 {
        frame(&mut app, &[], T * 5);
    }
    assert_eq!(tick_of(&app) - start, 20);
    assert_eq!(frame(&mut app, &[speed(Speed::X1)], T), 1);
    assert_eq!(virt(&app).relative_speed(), 1.0);
}

#[test]
fn halt_causes_are_independent() {
    let mut app = small();
    assert_eq!(frame(&mut app, &[pause(), occluded(true)], T), 0);
    assert_eq!(frame(&mut app, &[occluded(false)], T), 0);
    assert!(clock(&app).paused());
    assert!(!clock(&app).hidden());
    assert_eq!(frame(&mut app, &[], T), 0);
    assert_eq!(frame(&mut app, &[resume()], T), 0);
    assert_eq!(frame(&mut app, &[], T), 1);
}

#[test]
fn clock_reports_requests() {
    let mut app = small();
    let c = clock(&app);
    assert_eq!(c, Clock::default());
    assert_eq!(c.speed(), Speed::X1);
    assert!(!c.paused() && !c.hidden() && !c.halted());
    frame(&mut app, &[speed(Speed::X2), speed(Speed::X1)], T);
    assert_eq!(clock(&app).speed(), Speed::X1);
    frame(&mut app, &[speed(Speed::X2)], T);
    assert_eq!(clock(&app).speed(), Speed::X2);
    assert_eq!(frame(&mut app, &[resume()], T), 2);
}

#[test]
fn idle_clock_leaves_the_driver_alone() {
    let mut app = small();
    for _ in 0..10 {
        assert_eq!(frame(&mut app, &[], T), 1);
    }
    assert_eq!(frame(&mut app, &[], T * 5), 2);
    assert_eq!(frame(&mut app, &[], T - NS), 0);
    assert_eq!(frame(&mut app, &[], secs(10)), 2);
    assert_eq!(overstep(&app), T - NS);
}
