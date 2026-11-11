#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::WindowEvent;
use salmong_core::balance::Balance;
use salmong_core::ids::{BusId, PassengerId, RouteId, StationId, Tick};
use salmong_core::replay::hash_sequence;
use salmong_core::scenario::{Scenario, Shape, StationSpec};
use salmong_core::sim::{BusMode, BusPhase, Command, Sim, SimEvent, SimMode};
use salmong_game::bus::{BusPlugin, BusTrack, bus_pos};
use salmong_game::clock::{Clock, ClockCommand, ClockPlugin};
use salmong_game::demo::{self, CHECK_EVERY, Demo, DemoPlugin, END_TICK};
use salmong_game::edits::{
    self, EDIT_FRAME, Edits, EditsPlugin, HOLD_FRAMES, PAUSE_TICK, Tally, bound_line, paused_line,
    stats_prefix,
};
use salmong_game::input::{CommandLog, InputPlugin, PendingCommands, SimRes};
use salmong_game::pack::PackPlugin;
use salmong_game::render::{BusDraw, RenderPlugin, RouteDraw, Scene};
use salmong_game::sim::{ReplayFeed, SimEventMessage, SimPlugin, step_sim};
use salmong_pack::parse_pack;

const PACK: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");
const EPS: f32 = 1e-3;

#[derive(Resource, Default)]
struct Hashes(Vec<u64>);

#[derive(Resource, Default)]
struct Spawned(u64);

fn record(sim: Res<SimRes>, mut hashes: ResMut<Hashes>) {
    hashes.0.push(sim.sim.state_hash());
}

fn count_spawned(mut reader: MessageReader<SimEventMessage>, mut n: ResMut<Spawned>) {
    n.0 += reader
        .read()
        .filter(|m| matches!(m.0, SimEvent::Spawned { .. }))
        .count() as u64;
}

fn scenario() -> Scenario {
    parse_pack(PACK).expect("pack").scenarios[0].clone()
}

fn step() -> Duration {
    Duration::from_nanos(33_333_333)
}

fn base_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<WindowEvent>()
        .add_plugins(AssetPlugin::default())
        .init_asset::<Mesh>()
        .init_asset::<ColorMaterial>()
        .add_plugins((InputPlugin, SimPlugin, BusPlugin, ClockPlugin, RenderPlugin))
        .init_resource::<Hashes>()
        .init_resource::<Spawned>()
        .add_systems(
            FixedUpdate,
            record.after(step_sim).run_if(resource_exists::<SimRes>),
        )
        .add_systems(Update, count_spawned);
    app.world_mut().spawn(Camera2d);
    app
}

fn harness(feed: Vec<(Tick, Command)>) -> App {
    let mut app = base_app();
    let sc = scenario();
    app.insert_resource(SimRes {
        sim: Sim::new(sc.clone(), u64::from(sc.seed), Balance::mvp()),
        scenario: sc,
    });
    app.insert_resource(ReplayFeed::new(feed));
    app
}

fn tick_of(app: &App) -> u64 {
    app.world().resource::<SimRes>().sim.view().tick.0
}

fn alpha(app: &App) -> f32 {
    app.world().resource::<Time<Fixed>>().overstep_fraction()
}

fn drawn(app: &App) -> Vec<(BusId, Vec2)> {
    app.world()
        .resource::<BusDraw>()
        .buses
        .iter()
        .map(|b| (b.id, b.pos))
        .collect()
}

fn frame(app: &mut App, d: Duration) -> u64 {
    let before = tick_of(app);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(d));
    app.update();
    tick_of(app) - before
}

struct Reference {
    pos: Vec<Vec<(BusId, Vec2)>>,
}

impl Reference {
    fn new(body: &[(Tick, Command)]) -> Self {
        let sc = scenario();
        let travel = Balance::mvp().travel_ticks;
        let mut sim = Sim::new(sc.clone(), u64::from(sc.seed), Balance::mvp());
        let snap = |sim: &Sim| -> Vec<(BusId, Vec2)> {
            sim.view()
                .buses
                .iter()
                .map(|b| (b.id, bus_pos(b, &sc.stations, travel)))
                .collect()
        };
        let mut pos = vec![snap(&sim)];
        let mut events = Vec::new();
        let mut bi = 0;
        for t in 0..=END_TICK + 2 {
            let before = sim.view();
            while bi < body.len() && body[bi].0.0 == t {
                sim.queue(body[bi].1.clone());
                bi += 1;
            }
            sim.step(&mut events);
            let after = sim.view();
            for b in &after.buses {
                let Some(bb) = before.buses.iter().find(|x| x.id == b.id) else {
                    continue;
                };
                let d =
                    (bus_pos(b, &sc.stations, travel) - bus_pos(bb, &sc.stations, travel)).length();
                let bound = match bb.phase {
                    BusPhase::Moving { from, to, .. } => {
                        let c = |s: StationId| {
                            let st = &sc.stations[usize::from(s.0)];
                            Vec2::new(f32::from(st.x), f32::from(st.y))
                        };
                        (c(to) - c(from)).length() / travel as f32
                    }
                    _ => 0.0,
                };
                assert!(
                    d <= bound + EPS,
                    "bus {:?} moved {d} > {bound} at tick {t}",
                    b.id
                );
            }
            pos.push(snap(&sim));
        }
        Self { pos }
    }

    fn s(&self, k: u64, id: BusId) -> f32 {
        let k = k as usize;
        if k == 0 {
            return 0.0;
        }
        let cur = self.pos[k].iter().find(|(i, _)| *i == id);
        let prev = self.pos[k - 1].iter().find(|(i, _)| *i == id);
        match (prev, cur) {
            (Some(&(_, p)), Some(&(_, c))) => (c - p).length(),
            _ => 0.0,
        }
    }

    fn allowed(&self, id: BusId, t0: u64, a0: f32, t1: u64, a1: f32) -> f32 {
        if t0 == t1 {
            return (a1 - a0).abs() * self.s(t1, id);
        }
        let mut total = (1.0 - a0) * self.s(t0, id) + a1 * self.s(t1, id);
        for k in t0 + 1..t1 {
            total += self.s(k, id);
        }
        total
    }
}

struct Watch {
    t: u64,
    a: f32,
    pos: Vec<(BusId, Vec2)>,
    checked: usize,
}

impl Watch {
    fn new(app: &App) -> Self {
        Self {
            t: tick_of(app),
            a: alpha(app),
            pos: drawn(app),
            checked: 0,
        }
    }

    fn check(&mut self, app: &App, r: &Reference) {
        let (t1, a1, pos) = (tick_of(app), alpha(app), drawn(app));
        for &(id, p1) in &pos {
            let Some(&(_, p0)) = self.pos.iter().find(|(i, _)| *i == id) else {
                continue;
            };
            let d = (p1 - p0).length();
            let allowed = r.allowed(id, self.t, self.a, t1, a1);
            assert!(
                d <= allowed + EPS,
                "bus {id:?} drawn step {d} > {allowed} from ({}, {}) to ({t1}, {a1})",
                self.t,
                self.a
            );
            self.checked += 1;
        }
        self.t = t1;
        self.a = a1;
        self.pos = pos;
    }
}

fn run_bounded(frames: impl Iterator<Item = Duration>) -> [bool; 3] {
    let body = edits::body();
    let r = Reference::new(&body);
    let mut app = harness(body);
    frame(&mut app, Duration::ZERO);
    let mut w = Watch::new(&app);
    let mut kinds = [false; 3];
    for d in frames {
        if tick_of(&app) >= END_TICK {
            break;
        }
        let n = frame(&mut app, d);
        kinds[n as usize] = true;
        w.check(&app, &r);
    }
    assert!(tick_of(&app) >= END_TICK);
    assert!(w.checked > 300, "only {} checks", w.checked);
    kinds
}

#[test]
fn drawn_step_bounded_through_edits() {
    let mut seen = [false; 3];
    let ms = [1u64, 40, 7, 16, 70, 33, 2, 120, 16, 0, 250, 9];
    let rates: Vec<Box<dyn Iterator<Item = Duration>>> = vec![
        Box::new(std::iter::repeat(step() / 2)),
        Box::new(std::iter::repeat(Duration::from_secs_f64(1.0 / 144.0))),
        Box::new(std::iter::repeat(step() * 3)),
        Box::new(ms.iter().cycle().map(|&m| Duration::from_millis(m))),
    ];
    for rate in rates {
        let k = run_bounded(rate);
        for i in 0..3 {
            seen[i] |= k[i];
        }
    }
    assert_eq!(seen, [true, true, true]);
}

#[test]
fn drawn_riders_conserved_every_frame() {
    let mut app = harness(edits::body());
    let mut with_riders = 0;
    while tick_of(&app) < END_TICK {
        frame(&mut app, step() / 2);
        let w = app.world();
        let view = w.resource::<SimRes>().sim.view();
        let draw = w.resource::<BusDraw>();
        let scene = w.resource::<Scene>();
        let riders: u64 = draw.buses.iter().map(|b| u64::from(b.riders)).sum();
        let badges: u64 = scene.badges.iter().map(|b| u64::from(b.extra)).sum();
        assert_eq!(
            riders + scene.passengers.len() as u64 + badges + view.score,
            w.resource::<Spawned>().0,
            "tick {}",
            view.tick.0
        );
        assert_eq!(draw.buses.len(), view.buses.len());
        for b in &draw.buses {
            let vb = view
                .buses
                .iter()
                .find(|x| x.id == b.id)
                .expect("drawn bus in view");
            assert_eq!(b.riders as usize, vb.riders.len());
        }
        with_riders += usize::from(riders > 0);
    }
    assert!(with_riders >= 50, "only {with_riders} frames with riders");
}

fn paused_cmd() -> Command {
    edits::body()
        .into_iter()
        .find(|(t, _)| t.0 == PAUSE_TICK)
        .map(|(_, c)| c)
        .unwrap()
}

#[test]
fn paused_edit_binds_to_first_tick_after_resume() {
    let body = edits::body();
    let r = Reference::new(&body);
    let fed: Vec<(Tick, Command)> = body
        .iter()
        .filter(|(t, _)| t.0 != PAUSE_TICK)
        .cloned()
        .collect();
    let mut app = harness(fed);
    frame(&mut app, Duration::ZERO);
    let mut w = Watch::new(&app);
    while tick_of(&app) < PAUSE_TICK {
        frame(&mut app, step() / 2);
        w.check(&app, &r);
    }
    assert_eq!(tick_of(&app), PAUSE_TICK);
    frame(&mut app, step() / 4);
    w.check(&app, &r);
    assert_eq!(tick_of(&app), PAUSE_TICK);
    assert!(alpha(&app) > 0.0);
    app.world_mut().write_message(ClockCommand::Pause);
    for _ in 0..10 {
        frame(&mut app, step() / 2);
        w.check(&app, &r);
    }
    assert!(app.world().resource::<Clock>().paused());
    let hash = app.world().resource::<SimRes>().sim.state_hash();
    let log_len = app.world().resource::<CommandLog>().0.len();
    let routes = app.world().resource::<RouteDraw>().clone();
    let rev = app.world().resource::<SimRes>().sim.view().table_rev;
    app.world_mut()
        .resource_mut::<PendingCommands>()
        .0
        .push(paused_cmd());
    for _ in 0..30 {
        frame(&mut app, step() / 2);
        w.check(&app, &r);
        let wd = app.world();
        assert_eq!(tick_of(&app), PAUSE_TICK);
        assert_eq!(wd.resource::<SimRes>().sim.state_hash(), hash);
        assert_eq!(wd.resource::<CommandLog>().0.len(), log_len);
        assert_eq!(*wd.resource::<RouteDraw>(), routes);
        assert_eq!(wd.resource::<SimRes>().sim.view().table_rev, rev);
        assert_eq!(wd.resource::<PendingCommands>().0.len(), 1);
    }
    app.world_mut().write_message(ClockCommand::Resume);
    let mut guard = 0;
    while tick_of(&app) == PAUSE_TICK {
        frame(&mut app, step() / 2);
        w.check(&app, &r);
        guard += 1;
        assert!(guard < 10);
    }
    let wd = app.world();
    assert_eq!(
        wd.resource::<CommandLog>().0.last(),
        Some(&(Tick(PAUSE_TICK), paused_cmd()))
    );
    assert!(wd.resource::<PendingCommands>().0.is_empty());
    assert_ne!(wd.resource::<RouteDraw>().rev, routes.rev);
    while tick_of(&app) < END_TICK {
        frame(&mut app, step() / 2);
        w.check(&app, &r);
    }
    let sc = scenario();
    let expected = hash_sequence(
        sc.clone(),
        u64::from(sc.seed),
        Balance::mvp(),
        &body,
        END_TICK + 1,
    );
    let hashes = &app.world().resource::<Hashes>().0;
    assert!(hashes.len() >= END_TICK as usize);
    assert_eq!(hashes[..], expected[1..=hashes.len()]);
}

#[test]
fn edits_body_has_riders_on_board_at_every_edit() {
    let sc = scenario();
    let body = edits::body();
    assert_eq!(body.iter().filter(|(t, _)| t.0 == PAUSE_TICK).count(), 1);
    assert!(!PAUSE_TICK.is_multiple_of(CHECK_EVERY));
    let edit_ticks: Vec<u64> = body
        .iter()
        .filter(|(_, c)| {
            matches!(
                c,
                Command::EditRoute {
                    route: RouteId(0),
                    ..
                } | Command::RemoveRoute { route: RouteId(0) }
            )
        })
        .map(|(t, _)| t.0)
        .collect();
    assert_eq!(edit_ticks.len(), 4);
    let mut sim = Sim::new(sc.clone(), u64::from(sc.seed), Balance::mvp());
    let mut events = Vec::new();
    let mut bi = 0;
    let mut moving = 0;
    for t in 0..END_TICK {
        if edit_ticks.contains(&t) {
            let v = sim.view();
            let on: Vec<_> = v
                .buses
                .iter()
                .filter(|b| matches!(b.mode, BusMode::Active { route, .. } if route == RouteId(0)))
                .collect();
            let riders: usize = on.iter().map(|b| b.riders.len()).sum();
            let is_moving = on
                .iter()
                .any(|b| !b.riders.is_empty() && matches!(b.phase, BusPhase::Moving { .. }));
            println!("edit tick {t}: route 0 riders {riders}, moving with riders {is_moving}");
            assert!(riders >= 1, "no rider on route 0 at edit tick {t}");
            moving += usize::from(is_moving);
        }
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        sim.step(&mut events);
    }
    assert!(moving >= 2, "only {moving} edits with a moving loaded bus");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SimEvent::CommandRejected))
    );
    assert_eq!(sim.view().mode, SimMode::Running);
}

fn edits_run(first: Duration, rest: Duration) -> App {
    let mut app = base_app();
    app.add_plugins((PackPlugin, DemoPlugin, EditsPlugin))
        .add_systems(PostStartup, edits::start);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.update();
    assert!(app.world().contains_resource::<Edits>());
    frame(&mut app, first);
    while tick_of(&app) < END_TICK {
        frame(&mut app, rest);
        let held = app.world().resource::<Edits>().held;
        if held > 0 && held < HOLD_FRAMES {
            assert_eq!(tick_of(&app), PAUSE_TICK, "tick moved during the hold");
        }
    }
    app
}

fn assert_edits(app: &mut App) {
    let expect = edits::expect();
    let lines = app.world().resource::<Demo>().lines.clone();
    assert_eq!(lines.len(), 15, "{lines:#?}");
    assert_eq!(expect.len(), 15);
    for (l, e) in lines.iter().zip(&expect) {
        assert!(l.starts_with(e.as_str()), "{l} vs {e}");
    }
    let sc = scenario();
    let hs = hash_sequence(
        sc.clone(),
        u64::from(sc.seed),
        Balance::mvp(),
        &edits::body(),
        END_TICK + 1,
    );
    for k in 1..=6u64 {
        let t = CHECK_EVERY * k;
        let want = format!(
            "salmong: replay tick={t} hash={:016x} frame=",
            hs[t as usize]
        );
        assert!(lines.iter().any(|l| l.starts_with(&want)), "missing {want}");
    }
    let stats: Vec<&String> = lines
        .iter()
        .filter(|l| l.starts_with("salmong: replay edits "))
        .collect();
    assert_eq!(stats.len(), 6);
    for s in stats {
        assert!(s.ends_with("draw_over=0 lost=0"), "{s}");
    }
    let paused = lines
        .iter()
        .find(|l| l.starts_with("salmong: replay paused"))
        .unwrap();
    assert!(
        paused.contains("tick=390 ") && paused.contains(" pending=1 "),
        "{paused}"
    );
    let bound = lines
        .iter()
        .find(|l| l.starts_with("salmong: replay bound"))
        .unwrap();
    assert!(
        bound.contains("tick=390 ") && bound.contains(" pending=0 "),
        "{bound}"
    );
    assert_eq!(app.world().resource::<Edits>().held, HOLD_FRAMES);
    for _ in 0..20 {
        frame(app, step() * 3);
        assert_eq!(tick_of(app), END_TICK);
    }
    assert!(app.world().resource::<Clock>().paused());
    assert_eq!(alpha(app), 0.0);
    assert_eq!(app.world().resource::<Demo>().lines.len(), 15);
}

#[test]
fn edits_lines_match_expect_and_freeze() {
    for (first, rest) in [
        (step() / 2, step() / 2),
        (step(), step() * 3),
        (step() * 3, step() * 3),
    ] {
        let mut app = edits_run(first, rest);
        assert_edits(&mut app);
    }
}

#[test]
fn hand_values() {
    assert!(demo::selects("?replay=edits", "edits"));
    assert!(demo::selects("a=1&replay=edits", "edits"));
    assert!(!demo::selects("?replay=edits2", "edits"));
    assert!(!demo::selects("?replay=demo", "edits"));
    assert!(demo::selects_demo("?replay=demo"));
    assert_eq!(
        paused_line(390, 2, 1, 77),
        "salmong: replay paused tick=390 rev=2 pending=1 frame=77"
    );
    assert_eq!(
        bound_line(390, 3, 0, 300),
        "salmong: replay bound tick=390 rev=3 pending=0 frame=300"
    );
    let mut track = BusTrack::new(10);
    track.push(
        Tick(1),
        vec![
            (BusId(0), Vec2::new(0.0, 0.0)),
            (BusId(1), Vec2::new(5.0, 5.0)),
        ],
    );
    track.push(
        Tick(2),
        vec![
            (BusId(0), Vec2::new(3.0, 4.0)),
            (BusId(1), Vec2::new(5.0, 6.0)),
            (BusId(2), Vec2::new(40.0, 40.0)),
        ],
    );
    let spawn = |i: u32| SimEvent::Spawned {
        passenger: PassengerId(i),
        station: StationId(0),
        want: Shape::Circle,
    };
    let events = vec![
        spawn(0),
        SimEvent::Boarded {
            bus: BusId(0),
            passenger: PassengerId(0),
            station: StationId(0),
        },
        spawn(1),
        SimEvent::CommandRejected,
    ];
    let mut tally = Tally::default();
    tally.tick(&events, &track);
    assert_eq!(
        tally,
        Tally {
            spawned: 2,
            rejected: 1,
            step_milli: 5000
        }
    );
    let sc = scenario();
    let sim = Sim::new(sc.clone(), u64::from(sc.seed), Balance::mvp());
    let _: &[StationSpec] = &sc.stations;
    assert_eq!(
        stats_prefix(&sim.view(), &tally),
        "salmong: replay edits tick=0 riders=0 queued=0 delivered=0 spawned=2 rejected=1 step=5000"
    );
    assert_eq!(EDIT_FRAME, 60);
}
