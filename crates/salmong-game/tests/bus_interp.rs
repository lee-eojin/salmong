#![cfg(not(target_arch = "wasm32"))]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::WindowEvent;
use salmong_core::balance::Balance;
use salmong_core::ids::{BusId, RouteId, StationId, Tick};
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{BusMode, BusPhase, BusView, Command, Sim, SimEvent, SimMode, SimView};
use salmong_game::bus::{BusPlugin, BusTrack, blend, bus_pos};
use salmong_game::input::{InputPlugin, SimRes};
use salmong_game::sim::{ReplayFeed, SimEventMessage, SimPlugin};

const EPS: f32 = 1e-3;
const TRAVEL: u32 = 10;

#[derive(Resource, Default)]
struct Seen(Vec<SimEvent>);

fn collect(mut reader: MessageReader<SimEventMessage>, mut seen: ResMut<Seen>) {
    seen.0.extend(reader.read().map(|m| m.0.clone()));
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Disjoint,
    Reverse,
    Remove,
}

fn spec(x: u16, y: u16, shape: Shape) -> StationSpec {
    StationSpec {
        x,
        y,
        shape,
        at_tick: 0,
        weight: 1,
    }
}

fn five() -> Scenario {
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint {
            tick: 0,
            rate: 1500,
        }],
        stations: vec![
            spec(8, 8, Shape::Circle),
            spec(24, 10, Shape::Triangle),
            spec(40, 8, Shape::Square),
            spec(20, 30, Shape::Star),
            spec(44, 28, Shape::Circle),
        ],
    }
}

fn stops(s: &[u16]) -> Vec<StationId> {
    s.iter().map(|&i| StationId(i)).collect()
}

fn body(kind: Kind, t: u64) -> Vec<(Tick, Command)> {
    let edit = match kind {
        Kind::Disjoint => Command::EditRoute {
            route: RouteId(0),
            stops: stops(&[4, 3]),
        },
        Kind::Reverse => Command::EditRoute {
            route: RouteId(0),
            stops: stops(&[2, 1, 0]),
        },
        Kind::Remove => Command::RemoveRoute { route: RouteId(0) },
    };
    vec![
        (
            Tick(0),
            Command::AddRoute {
                stops: stops(&[0, 1, 2]),
            },
        ),
        (
            Tick(0),
            Command::AddRoute {
                stops: stops(&[3, 4]),
            },
        ),
        (Tick(t), edit),
    ]
}

fn harness(scenario: Scenario, feed: Option<Vec<(Tick, Command)>>) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<WindowEvent>()
        .add_plugins((InputPlugin, SimPlugin, BusPlugin))
        .init_resource::<Seen>()
        .add_systems(Update, collect);
    let seed = u64::from(scenario.seed);
    app.insert_resource(SimRes {
        sim: Sim::new(scenario.clone(), seed, Balance::mvp()),
        scenario,
    });
    if let Some(b) = feed {
        app.insert_resource(ReplayFeed::new(b));
    }
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.update();
    app
}

fn view(app: &App) -> SimView {
    app.world().resource::<SimRes>().sim.view()
}

fn track(app: &App) -> &BusTrack {
    app.world().resource::<BusTrack>()
}

fn frame(app: &mut App, d: Duration) -> u64 {
    let before = view(app).tick.0;
    app.insert_resource(TimeUpdateStrategy::ManualDuration(d));
    app.update();
    view(app).tick.0 - before
}

fn timestep(app: &App) -> Duration {
    app.world().resource::<Time<Fixed>>().timestep()
}

fn c(st: &[StationSpec], s: StationId) -> Vec2 {
    let x = &st[s.0 as usize];
    Vec2::new(x.x as f32, x.y as f32)
}

fn pos(b: &BusView, st: &[StationSpec]) -> Vec2 {
    if let BusPhase::Moving { from, to, progress } = b.phase {
        let f = progress as f32 / TRAVEL as f32;
        Vec2::new(
            c(st, from).x * (1.0 - f) + c(st, to).x * f,
            c(st, from).y * (1.0 - f) + c(st, to).y * f,
        )
    } else {
        c(st, b.at)
    }
}

fn find(list: &[(BusId, Vec2)], id: BusId) -> Option<Vec2> {
    list.iter().find(|(i, _)| *i == id).map(|&(_, p)| p)
}

fn check_tick(app: &App, before: &SimView, last_cur: &[(BusId, Vec2)], st: &[StationSpec]) {
    let v = view(app);
    let tr = track(app);
    assert_eq!(tr.tick(), v.tick);
    let mapped: Vec<(BusId, Vec2)> = v
        .buses
        .iter()
        .map(|b| (b.id, bus_pos(b, st, TRAVEL)))
        .collect();
    assert_eq!(tr.cur(), &mapped[..]);
    for (b, (_, q)) in v.buses.iter().zip(tr.cur()) {
        assert!(
            (pos(b, st) - *q).length() <= EPS,
            "bus_pos disagrees with the reference"
        );
    }
    assert_eq!(tr.prev(), last_cur);
    for b in &v.buses {
        let Some(bb) = before.buses.iter().find(|x| x.id == b.id) else {
            continue;
        };
        let d = (pos(b, st) - pos(bb, st)).length();
        let bound = match bb.phase {
            BusPhase::Moving { from, to, .. } => (c(st, to) - c(st, from)).length() / TRAVEL as f32,
            _ => 0.0,
        };
        assert!(
            d <= bound + EPS,
            "bus {:?} moved {d} > {bound} at {:?}",
            b.id,
            v.tick
        );
        if let BusPhase::Moving { from, to, progress } = bb.phase {
            let next = BusPhase::Moving {
                from,
                to,
                progress: progress + 1,
            };
            if b.phase != next {
                assert_eq!(progress + 1, TRAVEL, "segment changed before it finished");
                assert_eq!(b.at, to);
                if let BusPhase::Moving {
                    from: f2,
                    progress: p2,
                    ..
                } = b.phase
                {
                    assert_eq!(
                        (f2, p2),
                        (to, 0),
                        "departure must start at the arrival stop"
                    );
                }
            }
        }
    }
    for alpha in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
        let at = tr.at(alpha);
        assert_eq!(at.len(), v.buses.len());
        for &(id, q) in &at {
            let cc = find(tr.cur(), id).unwrap();
            match find(tr.prev(), id) {
                None => assert_eq!(q, cc),
                Some(p) => {
                    let span = (cc - p).length();
                    assert!((q - p).length() + (q - cc).length() <= span + EPS);
                    assert!(((q - p).length() - alpha * span).abs() <= EPS);
                }
            }
        }
    }
    let seen = &app.world().resource::<Seen>().0;
    let spawned = seen
        .iter()
        .filter(|e| matches!(e, SimEvent::Spawned { .. }))
        .count();
    let riders: usize = v.buses.iter().map(|b| b.riders.len()).sum();
    assert_eq!(
        riders + v.queued + v.score as usize,
        spawned,
        "conservation at {:?}",
        v.tick
    );
    assert_eq!(v.queued, v.queues.iter().map(|q| q.len()).sum::<usize>());
    assert_eq!(v.free_buses as usize + v.buses.len(), 4);
}

fn on_route0(b: &BusView) -> bool {
    matches!(
        b.mode,
        BusMode::Active {
            route: RouteId(0),
            ..
        }
    )
}

#[derive(Default, Debug)]
struct Counts {
    moving_riders: u32,
    stopped_riders: u32,
    special: u32,
    scored: u32,
}

fn sweep(kind: Kind) -> Counts {
    let sc = five();
    let st = sc.stations.clone();
    let mut counts = Counts::default();
    for t in 40..130u64 {
        let mut app = harness(sc.clone(), Some(body(kind, t)));
        let ts = timestep(&app);
        let mut last_cur: Vec<(BusId, Vec2)> = track(&app).cur().to_vec();
        let mut special = false;
        let mut dir_at_edit = None;
        while view(&app).tick.0 < t + 160 {
            let before = view(&app);
            if before.tick.0 == t {
                let r0: Vec<&BusView> = before.buses.iter().filter(|b| on_route0(b)).collect();
                if r0
                    .iter()
                    .any(|b| matches!(b.phase, BusPhase::Moving { .. }) && !b.riders.is_empty())
                {
                    counts.moving_riders += 1;
                }
                if r0
                    .iter()
                    .any(|b| !matches!(b.phase, BusPhase::Moving { .. }) && !b.riders.is_empty())
                {
                    counts.stopped_riders += 1;
                }
                dir_at_edit = r0.first().map(|b| (b.id, b.mode.clone()));
            }
            assert_eq!(frame(&mut app, ts), 1);
            check_tick(&app, &before, &last_cur, &st);
            last_cur = track(&app).cur().to_vec();
            let v = view(&app);
            if v.tick.0 > t {
                match kind {
                    Kind::Disjoint => {
                        special |= v.buses.iter().any(|b| {
                            matches!(b.phase, BusPhase::Moving { from, to, .. } if from.0 <= 2 && to.0 >= 3)
                        })
                    }
                    Kind::Reverse => {
                        if let Some((id, mode)) = &dir_at_edit
                            && v.tick.0 == t + 1
                            && let Some(b) = v.buses.iter().find(|b| b.id == *id)
                        {
                            special |= b.mode == *mode
                                && matches!(b.phase, BusPhase::Moving { .. });
                        }
                    }
                    Kind::Remove => {
                        special |= v.buses.iter().any(|b| {
                            b.mode == BusMode::Retiring
                                && matches!(b.phase, BusPhase::Moving { .. })
                                && !b.riders.is_empty()
                        })
                    }
                }
            }
        }
        let v = view(&app);
        assert_eq!(v.mode, SimMode::Running);
        assert_eq!(app.world().resource::<ReplayFeed>().remaining(), 0);
        let seen = &app.world().resource::<Seen>().0;
        assert!(!seen.iter().any(|e| matches!(e, SimEvent::CommandRejected)));
        assert!(seen.iter().any(|e| matches!(e, SimEvent::Boarded { .. })));
        if kind == Kind::Remove {
            assert_eq!(v.free_buses, 3);
        }
        if v.score > 0 {
            counts.scored += 1;
        }
        if special {
            counts.special += 1;
        }
    }
    println!("{kind:?} {counts:?}");
    assert!(counts.moving_riders >= 10, "{counts:?}");
    assert!(counts.stopped_riders >= 5, "{counts:?}");
    assert!(counts.special >= 10, "{counts:?}");
    assert!(counts.scored >= 1, "{counts:?}");
    counts
}

#[test]
fn disjoint_edit_sweep() {
    sweep(Kind::Disjoint);
}

#[test]
fn reverse_edit_sweep() {
    sweep(Kind::Reverse);
}

#[test]
fn remove_route_sweep() {
    sweep(Kind::Remove);
}

fn moving(from: u16, to: u16, progress: u32) -> BusView {
    BusView {
        id: BusId(0),
        mode: BusMode::Retiring,
        phase: BusPhase::Moving {
            from: StationId(from),
            to: StationId(to),
            progress,
        },
        at: StationId(from),
        last_arrival: None,
        riders: Vec::new(),
    }
}

#[test]
fn bus_pos_hand_values() {
    let st = vec![
        spec(0, 0, Shape::Circle),
        spec(10, 0, Shape::Triangle),
        spec(10, 20, Shape::Square),
    ];
    assert_eq!(bus_pos(&moving(0, 1, 0), &st, 10), Vec2::new(0.0, 0.0));
    assert_eq!(bus_pos(&moving(0, 1, 3), &st, 10), Vec2::new(3.0, 0.0));
    assert_eq!(bus_pos(&moving(0, 1, 9), &st, 10), Vec2::new(9.0, 0.0));
    assert_eq!(bus_pos(&moving(1, 2, 5), &st, 10), Vec2::new(10.0, 10.0));
    for phase in [BusPhase::Boarding, BusPhase::Alighting] {
        let mut b = moving(0, 1, 0);
        b.phase = phase;
        b.at = StationId(2);
        assert_eq!(bus_pos(&b, &st, 10), Vec2::new(10.0, 20.0));
    }
    assert_eq!(bus_pos(&moving(1, 2, 0), &st, 1), Vec2::new(10.0, 0.0));
}

#[test]
fn blend_snaps_clamps_and_lerps() {
    let p = Vec2::new(0.0, 0.0);
    let cur = Vec2::new(8.0, 4.0);
    for a in [-1.0, 0.0, 0.3, 1.0, 2.0] {
        assert_eq!(blend(None, cur, a), cur);
    }
    assert_eq!(blend(Some(p), cur, -1.0), p);
    assert_eq!(blend(Some(p), cur, 2.0), cur);
    assert_eq!(blend(Some(p), cur, 0.25), Vec2::new(2.0, 1.0));
}

#[test]
fn push_rotates_only_on_consecutive_ticks() {
    let d = BusTrack::default();
    assert_eq!(d.tick(), Tick(0));
    assert!(d.cur().is_empty() && d.prev().is_empty());
    let a = vec![(BusId(0), Vec2::new(1.0, 0.0))];
    let b = vec![(BusId(0), Vec2::new(2.0, 0.0))];
    let cc = vec![(BusId(0), Vec2::new(3.0, 0.0))];
    let mut t = BusTrack::new(10);
    t.push(Tick(1), a.clone());
    t.push(Tick(2), b.clone());
    assert_eq!(t.prev(), &a[..]);
    assert_eq!(t.cur(), &b[..]);
    t.push(Tick(2), b.clone());
    assert_eq!(t.prev(), &b[..]);
    t.push(Tick(7), cc.clone());
    assert_eq!(t.prev(), &cc[..]);
    assert_eq!(t.tick(), Tick(7));
}

#[test]
fn at_snaps_new_bus_and_drops_parked_bus() {
    let p0 = Vec2::new(0.0, 0.0);
    let p1 = Vec2::new(2.0, 2.0);
    let c1 = Vec2::new(4.0, 6.0);
    let c2 = Vec2::new(9.0, 9.0);
    let mut t = BusTrack::new(10);
    t.push(Tick(1), vec![(BusId(0), p0), (BusId(1), p1)]);
    t.push(Tick(2), vec![(BusId(1), c1), (BusId(2), c2)]);
    assert_eq!(
        t.at(0.5),
        vec![(BusId(1), Vec2::new(3.0, 4.0)), (BusId(2), c2)]
    );
}

#[test]
fn two_tick_frames_keep_adjacent_snapshots() {
    let sc = five();
    let st = sc.stations.clone();
    let b = body(Kind::Disjoint, 60);
    let mut app = harness(sc.clone(), Some(b.clone()));
    let mut plain = Sim::new(sc.clone(), u64::from(sc.seed), Balance::mvp());
    let mut events = Vec::new();
    let mut bi = 0;
    let mut prev_plain: Vec<(BusId, Vec2)> = Vec::new();
    let mut cur_plain: Vec<(BusId, Vec2)> = Vec::new();
    let mut saw_two = false;
    let ms = [1u64, 40, 7, 16, 70, 33, 2, 120, 16, 0, 250, 9];
    let mut i = 0;
    while view(&app).tick.0 < 300 {
        let ran = frame(&mut app, Duration::from_millis(ms[i % ms.len()]));
        i += 1;
        saw_two |= ran == 2;
        for _ in 0..ran {
            let t = plain.view().tick.0;
            while bi < b.len() && b[bi].0.0 == t {
                plain.queue(b[bi].1.clone());
                bi += 1;
            }
            plain.step(&mut events);
            prev_plain = std::mem::take(&mut cur_plain);
            cur_plain = plain
                .view()
                .buses
                .iter()
                .map(|x| (x.id, bus_pos(x, &st, TRAVEL)))
                .collect();
        }
        if ran > 0 {
            let tr = track(&app);
            assert_eq!(tr.cur(), &cur_plain[..]);
            assert_eq!(tr.prev(), &prev_plain[..]);
            let alpha = app.world().resource::<Time<Fixed>>().overstep_fraction();
            for (id, q) in tr.at(alpha) {
                let cc = find(tr.cur(), id).unwrap();
                if let Some(p) = find(tr.prev(), id) {
                    assert!((q - p).length() + (q - cc).length() <= (cc - p).length() + EPS);
                } else {
                    assert_eq!(q, cc);
                }
            }
        }
    }
    assert!(saw_two);
}

#[test]
fn no_sim_res_leaves_track_empty() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<WindowEvent>()
        .add_plugins((InputPlugin, SimPlugin, BusPlugin));
    for _ in 0..5 {
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_nanos(
            33_333_333,
        )));
        app.update();
    }
    let tr = app.world().resource::<BusTrack>();
    assert_eq!(tr.tick(), Tick(0));
    assert!(tr.cur().is_empty());
}
