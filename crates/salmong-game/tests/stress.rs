#![cfg(not(target_arch = "wasm32"))]

use std::sync::LazyLock;
use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{AppLifecycle, WindowEvent, WindowOccluded};
use salmong_core::balance::Balance;
use salmong_core::ids::{RouteId, Tick};
use salmong_core::scenario::Shape;
use salmong_core::sim::{BusMode, BusPhase, Command, Sim, SimEvent, SimMode};
use salmong_game::bus::{BusPlugin, BusTrack, bus_pos, track_buses};
use salmong_game::clock::{Clock, ClockCommand, ClockPlugin, Speed};
use salmong_game::input::{CommandLog, InputPlugin, SimRes};
use salmong_game::render::{BusDraw, QUEUE_CAP, RenderPlugin, buses, scene};
use salmong_game::sim::SimPlugin;
use salmong_game::stress::{
    CHECK_EVERY, EDIT_EVERY, EDIT_FIRST, END_TICK, HUB, Stress, StressPlugin, WARMUP_TICK, alive,
    body, expect, in_service, scenario, selected, start, stops,
};

const CAP: usize = 5_000;

struct Plain {
    peak: usize,
    cap_tick: Option<u64>,
    min_after_warmup: usize,
    rejected: usize,
    mode: SimMode,
    revs_after_warmup: usize,
    stars_at_warmup: usize,
    stars_at_end: usize,
    score_at_warmup: u64,
    score_at_end: u64,
    drawn_max: usize,
    service_ok: bool,
    service_detail: String,
}

fn stars(sim: &Sim) -> usize {
    sim.view()
        .queues
        .iter()
        .flatten()
        .filter(|p| p.want == Shape::Star)
        .count()
}

static PLAIN: LazyLock<Plain> = LazyLock::new(|| {
    let sc = scenario();
    let mut sim = Sim::new(sc.clone(), 1, Balance::bench());
    let body = body();
    let mut events = Vec::new();
    let mut bi = 0;
    let mut p = Plain {
        peak: 0,
        cap_tick: None,
        min_after_warmup: usize::MAX,
        rejected: 0,
        mode: SimMode::Running,
        revs_after_warmup: 0,
        stars_at_warmup: 0,
        stars_at_end: 0,
        score_at_warmup: 0,
        score_at_end: 0,
        drawn_max: 0,
        service_ok: true,
        service_detail: String::new(),
    };
    let mut last_rev = 0;
    for t in 0..END_TICK {
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        sim.step(&mut events);
        p.rejected += events
            .iter()
            .filter(|e| matches!(e, SimEvent::CommandRejected))
            .count();
        events.clear();
        let tick = t + 1;
        let v = sim.view();
        let a = alive(&v);
        p.peak = p.peak.max(a);
        if a >= CAP && p.cap_tick.is_none() {
            p.cap_tick = Some(tick);
        }
        if tick >= WARMUP_TICK {
            p.min_after_warmup = p.min_after_warmup.min(a);
            let ok = v.buses.len() == 8
                && in_service(&v) == 8
                && v.free_buses == 24
                && v.route_count == 8
                && v.buses.iter().all(|b| b.last_arrival.is_some());
            if !ok && p.service_ok {
                p.service_ok = false;
                p.service_detail = format!(
                    "tick {tick}: buses {} active {} free {} routes {}",
                    v.buses.len(),
                    in_service(&v),
                    v.free_buses,
                    v.route_count
                );
            }
            if tick > WARMUP_TICK && v.table_rev != last_rev {
                p.revs_after_warmup += 1;
            }
        }
        last_rev = v.table_rev;
        if tick == WARMUP_TICK {
            p.stars_at_warmup = stars(&sim);
            p.score_at_warmup = v.score;
        }
        if tick.is_multiple_of(CHECK_EVERY) {
            let s = scene(&v, &sc.stations);
            p.drawn_max = p.drawn_max.max(s.passengers.len() + s.badges.len());
        }
    }
    let v = sim.view();
    p.mode = v.mode;
    p.stars_at_end = stars(&sim);
    p.score_at_end = v.score;
    p
});

#[test]
fn selected_parses_query() {
    assert_eq!(selected("?bench=stress&seed=1"), Some(1));
    assert_eq!(selected("?seed=1&bench=stress"), Some(1));
    assert_eq!(selected("bench=stress&seed=7"), Some(7));
    for q in [
        "?bench=stress",
        "?bench=stress&seed=x",
        "?bench=stres&seed=1",
        "?replay=demo",
        "",
    ] {
        assert_eq!(selected(q), None, "{q}");
    }
}

#[test]
fn body_is_the_fixed_log() {
    let b = body();
    assert_eq!(b.len(), 32);
    assert!(b.windows(2).all(|w| w[0].0 <= w[1].0));
    for (r, (t, c)) in b.iter().take(8).enumerate() {
        assert_eq!(*t, Tick(0));
        assert_eq!(
            *c,
            Command::AddRoute {
                stops: stops(r, false)
            }
        );
    }
    for (k, (t, c)) in b.iter().skip(8).enumerate() {
        let k = k as u64;
        assert_eq!(*t, Tick(EDIT_FIRST + EDIT_EVERY * k));
        assert_eq!(
            *c,
            Command::EditRoute {
                route: RouteId(7),
                stops: stops(7, k.is_multiple_of(2))
            }
        );
    }
    assert!(stops(7, true).contains(&HUB));
    assert!(!stops(7, false).contains(&HUB));
    assert_eq!(stops(0, false).len(), 63);
}

#[test]
fn workload_reaches_cap_and_holds() {
    let p = &*PLAIN;
    assert_eq!(p.peak, CAP);
    let cap = p.cap_tick.expect("reaches the cap");
    assert!(cap <= WARMUP_TICK, "{cap}");
    assert!(p.min_after_warmup >= 4_900, "{}", p.min_after_warmup);
    assert_eq!(p.rejected, 0);
    assert_eq!(p.mode, SimMode::Running);
}

#[test]
fn eight_buses_in_service_through_steady() {
    let p = &*PLAIN;
    assert!(p.service_ok, "{}", p.service_detail);
}

#[test]
fn edits_apply_and_queues_stay_ineligible() {
    let p = &*PLAIN;
    assert_eq!(p.revs_after_warmup, 24);
    assert!(p.stars_at_warmup >= 1_500, "{}", p.stars_at_warmup);
    assert!(p.stars_at_end >= 2_500, "{}", p.stars_at_end);
    assert!(p.drawn_max <= 64 * QUEUE_CAP + 64, "{}", p.drawn_max);
    assert!(p.score_at_end > p.score_at_warmup);
}

type Positions = Vec<(salmong_core::ids::BusId, Vec2)>;

struct Track {
    at_checks: Vec<(u64, Positions)>,
    moving_at_checks: usize,
    before_end: Positions,
}

static TRACK: LazyLock<Track> = LazyLock::new(|| {
    let sc = scenario();
    let travel = Balance::bench().travel_ticks;
    let mut sim = Sim::new(sc.clone(), 1, Balance::bench());
    let body = body();
    let mut events = Vec::new();
    let mut bi = 0;
    let mut out = Track {
        at_checks: Vec::new(),
        moving_at_checks: 0,
        before_end: Vec::new(),
    };
    for t in 0..END_TICK {
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        sim.step(&mut events);
        events.clear();
        let tick = t + 1;
        if tick.is_multiple_of(CHECK_EVERY) || tick == END_TICK - 1 {
            let v = sim.view();
            let pos: Positions = v
                .buses
                .iter()
                .map(|b| (b.id, bus_pos(b, &sc.stations, travel)))
                .collect();
            if tick == END_TICK - 1 {
                out.before_end = pos;
                continue;
            }
            out.moving_at_checks += v
                .buses
                .iter()
                .filter(|b| matches!(b.phase, BusPhase::Moving { .. }))
                .count();
            out.at_checks.push((tick, pos));
        }
    }
    out
});

#[derive(Resource, Default)]
struct Tracked(Vec<(u64, Positions)>);

fn record_track(sim: Res<SimRes>, track: Res<BusTrack>, mut tracked: ResMut<Tracked>) {
    let tick = sim.sim.view().tick.0;
    if tick.is_multiple_of(CHECK_EVERY) {
        tracked.0.push((tick, track.at(1.0)));
    }
}

fn app(render: bool) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<WindowEvent>()
        .add_plugins((InputPlugin, SimPlugin, BusPlugin, ClockPlugin, StressPlugin));
    if render {
        app.add_plugins(AssetPlugin::default())
            .init_asset::<Mesh>()
            .add_plugins(RenderPlugin);
        app.world_mut().spawn(Camera2d);
    }
    app.init_resource::<Tracked>().add_systems(
        FixedUpdate,
        record_track
            .after(track_buses)
            .run_if(resource_exists::<SimRes>),
    );
    app.world_mut().run_system_once(start).expect("start");
    app
}

fn frame(app: &mut App, d: Duration) {
    app.insert_resource(TimeUpdateStrategy::ManualDuration(d));
    app.update();
}

fn tick_of(app: &App) -> u64 {
    app.world().resource::<SimRes>().sim.view().tick.0
}

fn bare(line: &str) -> String {
    match line.find(" frame=") {
        Some(i) => line[..i].to_string(),
        None => line.to_string(),
    }
}

fn frame_of(line: &str) -> u32 {
    line.rsplit("frame=").next().unwrap().parse().unwrap()
}

fn done(app: &App) -> bool {
    app.world()
        .resource::<Stress>()
        .lines
        .last()
        .is_some_and(|l| l.contains("bench done"))
}

fn run_rate(d: Duration) -> App {
    let mut app = app(true);
    frame(&mut app, Duration::ZERO);
    let mut n = 0;
    while !done(&app) {
        frame(&mut app, d);
        n += 1;
        assert!(n < 40_000, "no done line after {n} frames");
    }
    app
}

fn check_rate(d: Duration, expected: &[String]) -> App {
    let app = run_rate(d);
    let lines = &app.world().resource::<Stress>().lines;
    let got: Vec<String> = lines.iter().map(|l| bare(l)).collect();
    assert_eq!(got, expected, "rate {d:?}");
    assert!(
        lines
            .iter()
            .filter(|l| l.contains("bench tick="))
            .all(|l| l.contains(" buses=8 "))
    );
    assert_eq!(app.world().resource::<CommandLog>().0, body());
    assert_eq!(app.world().resource::<Clock>().speed(), Speed::X2);
    app
}

#[test]
fn app_lines_match_expect_at_rates() {
    let expected = expect(1);
    assert_eq!(expected.len(), 29);
    drop(check_rate(Duration::from_nanos(16_666_667), &expected));
    let mut app = check_rate(Duration::from_nanos(8_333_333), &expected);
    let lines = app.world().resource::<Stress>().lines.clone();
    let at = |tick: u64| {
        frame_of(
            lines
                .iter()
                .find(|l| l.starts_with(&format!("salmong: bench tick={tick} ")))
                .unwrap(),
        )
    };
    let steady = at(END_TICK) - at(WARMUP_TICK);
    assert!((14_398..=14_402).contains(&steady), "{steady}");
    let tick = tick_of(&app);
    assert_eq!(tick, END_TICK);
    let drawn = app.world().resource::<BusDraw>().clone();
    let reference = &*TRACK;
    assert!(reference.moving_at_checks > 0);
    assert_eq!(app.world().resource::<Tracked>().0, reference.at_checks);
    let track = reference.before_end.clone();
    let riders = app
        .world()
        .resource::<salmong_game::render::Scene>()
        .riders
        .clone();
    assert_eq!(drawn.buses, buses(&track, &riders));
    for _ in 0..30 {
        frame(&mut app, Duration::from_nanos(8_333_333));
    }
    assert_eq!(tick_of(&app), END_TICK);
    assert_eq!(*app.world().resource::<BusDraw>(), drawn);
    drop(check_rate(Duration::from_millis(100), &expected));
}

fn hash_of(line: &str) -> String {
    line.split(' ')
        .find(|w| w.starts_with("hash="))
        .unwrap()
        .to_string()
}

#[test]
fn transitions_keep_stress_hashes() {
    let expected: Vec<String> = expect(1)
        .iter()
        .filter(|l| l.contains("bench tick="))
        .map(|l| hash_of(l))
        .collect();
    let mut app = app(false);
    let t = Duration::from_nanos(33_333_333);
    frame(&mut app, Duration::ZERO);
    let occ = |o: bool| {
        WindowEvent::WindowOccluded(WindowOccluded {
            window: Entity::PLACEHOLDER,
            occluded: o,
        })
    };
    let mut n = 0u64;
    while !done(&app) {
        n += 1;
        let w = app.world_mut();
        let mut d = t;
        match n % 400 {
            10 => {
                w.write_message(ClockCommand::Pause);
                d = Duration::from_secs(10);
            }
            11 => {
                w.write_message(ClockCommand::Resume);
            }
            50 => d = Duration::from_millis(500),
            90 => {
                w.write_message(occ(true));
                d = Duration::from_secs(5);
            }
            91 => {
                w.write_message(occ(false));
                d = Duration::from_secs(10);
            }
            130 => {
                w.write_message(occ(true));
                w.write_message(occ(false));
                d = Duration::from_secs(10);
            }
            170 => {
                w.write_message(WindowEvent::AppLifecycle(AppLifecycle::WillSuspend));
            }
            171 => {
                w.write_message(WindowEvent::AppLifecycle(AppLifecycle::Suspended));
                d = Duration::from_secs(3);
            }
            172 => {
                w.write_message(WindowEvent::AppLifecycle(AppLifecycle::WillResume));
            }
            173 => {
                w.write_message(WindowEvent::AppLifecycle(AppLifecycle::Running));
            }
            210 => d = Duration::from_millis(7),
            211 => d = Duration::from_millis(70),
            _ => {}
        }
        frame(&mut app, d);
        assert!(n < 60_000, "no done line after {n} frames");
    }
    let got: Vec<String> = app
        .world()
        .resource::<Stress>()
        .lines
        .iter()
        .filter(|l| l.contains("bench tick="))
        .map(|l| hash_of(l))
        .collect();
    assert_eq!(got, expected);
    assert_eq!(tick_of(&app), END_TICK);
    assert!(
        app.world()
            .resource::<SimRes>()
            .sim
            .view()
            .buses
            .iter()
            .all(|b| matches!(b.mode, BusMode::Active { .. }))
    );
}
