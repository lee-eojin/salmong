#![cfg(not(target_arch = "wasm32"))]

use std::collections::BTreeSet;
use std::time::Duration;

use bevy::camera::{CameraProjection, ScalingMode};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::WindowEvent;
use salmong_core::balance::Balance;
use salmong_core::ids::{BusId, PassengerId, RouteId, StationId, Tick};
use salmong_core::input::{Fit, MILLI};
use salmong_core::replay::hash_sequence;
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{BusPhase, Command, Passenger, Sim, SimEvent, SimMode, SimView};
use salmong_game::bus::{BusPlugin, BusTrack};
use salmong_game::clock::{Clock, ClockCommand, ClockPlugin};
use salmong_game::demo::{self, CHECK_EVERY, Demo, DemoPlugin, END_TICK};
use salmong_game::input::{InputPlugin, SimRes};
use salmong_game::pack::{PackPlugin, ScenarioPack};
use salmong_game::render::{
    BADGE_COLOUR, BADGE_HALF, BUS_COLOUR, BUS_HALF, BusDraw, BusInst, Geometry, Layer,
    PASSENGER_COLOUR, PASSENGER_HALF, PassengerInst, QUEUE_CAP, RIDER_COLOUR, RIDER_HALF,
    ROUTE_HALF, RenderPlugin, RouteDraw, RouteInst, STATION_HALF, Scene, ShapeMaterial,
    StationInst, bus_geometry, buses, camera_fit, hit, passenger_geometry, route_colour,
    route_geometry, routes, scene, shape_code, shape_distance, slot_pos, station_colour,
    station_geometry, to_world,
};
use salmong_game::sim::{SimPlugin, step_sim};
use salmong_pack::parse_pack;

const PACK: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");
const N: usize = 600;

#[derive(Resource, Default)]
struct Hashes(Vec<u64>);

fn record(sim: Res<SimRes>, mut hashes: ResMut<Hashes>) {
    hashes.0.push(sim.sim.state_hash());
}

fn pack_scenario() -> Scenario {
    parse_pack(PACK).expect("pack").scenarios[0].clone()
}

fn stops(s: &[u16]) -> Vec<StationId> {
    s.iter().map(|&i| StationId(i)).collect()
}

fn driver_body() -> Vec<(Tick, Command)> {
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

fn base_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<WindowEvent>()
        .add_plugins(AssetPlugin::default())
        .init_asset::<Mesh>()
        .add_plugins((InputPlugin, SimPlugin, BusPlugin, ClockPlugin, RenderPlugin))
        .init_resource::<Hashes>()
        .add_systems(
            FixedUpdate,
            record.after(step_sim).run_if(resource_exists::<SimRes>),
        );
    app.world_mut().spawn(Camera2d);
    app
}

fn harness(scenario: Scenario, body: Vec<(Tick, Command)>) -> App {
    let mut app = base_app();
    let seed = u64::from(scenario.seed);
    app.insert_resource(SimRes {
        sim: Sim::new(scenario.clone(), seed, Balance::mvp()),
        scenario,
    });
    app.insert_resource(salmong_game::sim::ReplayFeed::new(body));
    app
}

fn tick_of(app: &App) -> u64 {
    app.world().resource::<SimRes>().sim.view().tick.0
}

fn frame(app: &mut App, d: Duration) -> u64 {
    let before = tick_of(app);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(d));
    app.update();
    tick_of(app) - before
}

fn step() -> Duration {
    Duration::from_nanos(33_333_333)
}

fn run_rate(frames: impl Iterator<Item = Duration>) -> Vec<u64> {
    let scenario = pack_scenario();
    let body = driver_body();
    let expected = hash_sequence(
        scenario.clone(),
        u64::from(scenario.seed),
        Balance::mvp(),
        &body,
        N as u64 + 1,
    );
    let mut app = harness(scenario.clone(), body);
    assert_eq!(frame(&mut app, Duration::ZERO), 0);
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
    let view = app.world().resource::<SimRes>().sim.view();
    assert_eq!(app.world().resource::<Scene>().tick, Some(view.tick));
    assert!(!app.world().resource::<Scene>().stations.is_empty());
    assert!(!app.world().resource::<RouteDraw>().routes.is_empty());
    counts
}

#[test]
fn hashes_match_headless_with_render_at_30hz() {
    let counts = run_rate(std::iter::repeat(step()));
    assert_eq!(counts.len(), N);
    assert!(counts.iter().all(|&c| c == 1), "{counts:?}");
}

#[test]
fn hashes_match_headless_with_render_at_60hz() {
    let counts = run_rate(std::iter::repeat(step() / 2));
    assert!(counts.iter().all(|&c| c <= 1));
    assert!(
        (2 * N..=2 * N + 2).contains(&counts.len()),
        "{}",
        counts.len()
    );
}

#[test]
fn hashes_match_headless_with_render_at_144hz() {
    let counts = run_rate(std::iter::repeat(Duration::from_secs_f64(1.0 / 144.0)));
    assert!(counts.iter().all(|&c| c <= 1));
    assert!(counts.len() > 4 * N, "{}", counts.len());
}

#[test]
fn hashes_match_headless_with_render_at_10hz_capped() {
    let counts = run_rate(std::iter::repeat(step() * 3));
    assert!(counts.iter().all(|&c| c == 2), "{counts:?}");
    assert_eq!(counts.len(), N / 2);
}

#[test]
fn hashes_match_headless_with_render_on_irregular_frames() {
    let ms = [1u64, 40, 7, 16, 70, 33, 2, 120, 16, 0, 250, 9];
    let counts = run_rate(ms.iter().cycle().map(|&m| Duration::from_millis(m)));
    assert!(counts.iter().all(|&c| c <= 2));
    for k in 0..=2 {
        assert!(counts.contains(&k), "no frame ran {k} ticks");
    }
}

fn demo_app() -> App {
    harness(pack_scenario(), demo::body())
}

#[test]
fn draw_data_matches_view_every_tick() {
    let mut app = demo_app();
    let stations = pack_scenario().stations;
    let (mut joined, mut edits, mut moving_riders) = (false, BTreeSet::new(), false);
    let mut last_count = 0;
    while tick_of(&app) < END_TICK {
        frame(&mut app, step() / 2);
        let w = app.world();
        let sim = &w.resource::<SimRes>().sim;
        let view = sim.view();
        let sc = w.resource::<Scene>();
        assert_eq!(*sc, scene(&view, &stations));
        assert_eq!(sc.tick, Some(view.tick));
        let expect_stations = if view.tick.0 > 600 { 4 } else { 3 };
        assert_eq!(sc.stations.len(), expect_stations, "tick {}", view.tick.0);
        for s in &sc.stations {
            let spec = &stations[usize::from(s.id.0)];
            assert_eq!(s.pos, Vec2::new(f32::from(spec.x), f32::from(spec.y)));
            assert_eq!(s.shape, spec.shape);
        }
        let rd = w.resource::<RouteDraw>();
        assert_eq!(rd.routes, routes(&sim.snapshot(), &stations));
        assert_eq!(rd.rev, Some(view.table_rev));
        if last_count == 3 && sc.stations.len() == 4 {
            joined = true;
        }
        last_count = sc.stations.len();
        edits.insert(format!("{:?}", rd.routes));
        moving_riders |= view
            .buses
            .iter()
            .any(|b| matches!(b.phase, BusPhase::Moving { .. }) && !b.riders.is_empty());
    }
    assert!(joined, "no station joined");
    assert!(edits.len() >= 3, "only {} route shapes", edits.len());
    assert!(moving_riders, "no moving bus with riders");
}

fn station(x: u16, y: u16, shape: Shape) -> StationSpec {
    StationSpec {
        x,
        y,
        shape,
        at_tick: 0,
        weight: 1,
    }
}

fn queue(n: usize) -> Vec<Passenger> {
    (0..n)
        .map(|i| Passenger {
            id: PassengerId(i as u32),
            want: Shape::ALL[i % 4],
        })
        .collect()
}

#[test]
fn queue_cap_and_badge_hand_table() {
    let specs: Vec<StationSpec> = (0..5)
        .map(|i| station(5 + 10 * i, 10, Shape::Circle))
        .collect();
    let sizes = [0usize, 1, 16, 17, 40];
    let view = SimView {
        tick: Tick(7),
        free_buses: 0,
        route_count: 0,
        table_rev: 0,
        score: 0,
        queued: sizes.iter().sum(),
        mode: SimMode::Running,
        overflow: vec![0; 5],
        queues: sizes.iter().map(|&n| queue(n)).collect(),
        buses: Vec::new(),
    };
    let sc = scene(&view, &specs);
    let per: Vec<usize> = (0..5)
        .map(|i| {
            sc.passengers
                .iter()
                .filter(|p| p.station == StationId(i))
                .count()
        })
        .collect();
    assert_eq!(per, vec![0, 1, 16, 16, 16]);
    let badges: Vec<(u16, u32)> = sc.badges.iter().map(|b| (b.station.0, b.extra)).collect();
    assert_eq!(badges, vec![(3, 1), (4, 24)]);
    for (i, &n) in sizes.iter().enumerate() {
        let centre = Vec2::new(f32::from(specs[i].x), 10.0);
        let mine: Vec<&PassengerInst> = sc
            .passengers
            .iter()
            .filter(|p| p.station == StationId(i as u16))
            .collect();
        for (k, p) in mine.iter().enumerate() {
            assert_eq!(usize::from(p.slot), k);
            assert_eq!(p.pos, slot_pos(centre, k));
            assert_eq!(p.want, view.queues[i][k].want);
        }
        if n > QUEUE_CAP {
            let b = sc
                .badges
                .iter()
                .find(|b| b.station == StationId(i as u16))
                .unwrap();
            assert_eq!(b.pos, slot_pos(centre, QUEUE_CAP));
        }
    }
    let crowded = Scenario {
        id: 0,
        seed: 3,
        w: 64,
        h: 40,
        ramp: vec![RampPoint {
            tick: 0,
            rate: 1500,
        }],
        stations: vec![
            station(12, 7, Shape::Circle),
            station(30, 22, Shape::Triangle),
            station(48, 12, Shape::Square),
        ],
    };
    let mut app = harness(
        crowded.clone(),
        vec![(
            Tick(0),
            Command::AddRoute {
                stops: stops(&[0, 1]),
            },
        )],
    );
    let mut badge = false;
    for _ in 0..3000 {
        frame(&mut app, step());
        let w = app.world();
        let sc = w.resource::<Scene>();
        let view = w.resource::<SimRes>().sim.view();
        assert_eq!(*sc, scene(&view, &crowded.stations));
        if sc
            .badges
            .iter()
            .any(|b| b.station == StationId(2) && b.extra > 0)
        {
            badge = true;
            break;
        }
        if view.mode == SimMode::Over {
            break;
        }
    }
    assert!(badge, "crowded replay never showed a badge");
}

#[test]
fn bus_draw_uses_overstep_fraction() {
    for d in [step() / 2, Duration::from_secs_f64(1.0 / 144.0)] {
        let mut app = demo_app();
        let mut mid = 0;
        while tick_of(&app) < 400 {
            frame(&mut app, d);
            let w = app.world();
            let track = w.resource::<BusTrack>();
            let alpha = w.resource::<Time<Fixed>>().overstep_fraction();
            let sc = w.resource::<Scene>();
            assert_eq!(
                w.resource::<BusDraw>().buses,
                buses(&track.at(alpha), &sc.riders)
            );
            if alpha > 0.0 && alpha < 1.0 && track.prev() != track.cur() {
                mid += 1;
            }
        }
        assert!(mid >= 50, "only {mid} blended frames");
    }
}

#[derive(Resource, Default)]
struct Changes {
    routes: usize,
    scene: usize,
    buses: usize,
}

fn count_changes(r: Res<RouteDraw>, s: Res<Scene>, b: Res<BusDraw>, mut c: ResMut<Changes>) {
    c.routes += usize::from(r.is_changed());
    c.scene += usize::from(s.is_changed());
    c.buses += usize::from(b.is_changed());
}

#[test]
fn routes_rebuild_on_network_change_only() {
    let mut app = demo_app();
    app.init_resource::<Changes>()
        .add_systems(Last, count_changes);
    let mut revs = 0;
    let mut last_rev = None;
    while tick_of(&app) < 700 {
        frame(&mut app, step() / 3);
        let rev = app.world().resource::<SimRes>().sim.view().table_rev;
        if last_rev != Some(rev) {
            revs += 1;
        }
        last_rev = Some(rev);
    }
    assert!(revs >= 4, "only {revs} revisions");
    assert_eq!(app.world().resource::<Changes>().routes, revs);
}

#[test]
fn scene_rebuilds_only_on_tick_frames() {
    let mut app = demo_app();
    app.init_resource::<Changes>()
        .add_systems(Last, count_changes);
    let mut tick_frames = 0;
    let mut frames = 0;
    let ms = [1u64, 40, 7, 16, 70, 33, 2, 120, 16, 0];
    for &m in ms.iter().cycle().take(400) {
        let ran = frame(&mut app, Duration::from_millis(m));
        frames += 1;
        tick_frames += usize::from(ran > 0 || frames == 1);
    }
    assert_eq!(app.world().resource::<Changes>().scene, tick_frames);
    app.world_mut().write_message(ClockCommand::Pause);
    let before = (
        app.world().resource::<Changes>().routes,
        app.world().resource::<Changes>().scene,
        app.world().resource::<Changes>().buses,
    );
    let t0 = tick_of(&app);
    for _ in 0..30 {
        frame(&mut app, step() / 2);
    }
    assert!(app.world().resource::<Clock>().paused());
    assert_eq!(tick_of(&app), t0);
    let c = app.world().resource::<Changes>();
    assert_eq!((c.routes, c.scene, c.buses), before);
}

fn world_to_pixel(p: Vec2, centre: Vec2, area: Rect, win: Vec2) -> Vec2 {
    Vec2::new(
        (p.x - centre.x - area.min.x) / area.width() * win.x,
        (area.max.y - (p.y - centre.y)) / area.height() * win.y,
    )
}

#[test]
fn camera_fit_agrees_with_input_fit() {
    let scenario = pack_scenario();
    let mut app = demo_app();
    frame(&mut app, step());
    let (proj, tf) = app
        .world_mut()
        .query_filtered::<(&Projection, &Transform), With<Camera2d>>()
        .single(app.world())
        .map(|(p, t)| (p.clone(), *t))
        .unwrap();
    let Projection::Orthographic(mut ortho) = proj else {
        panic!("not orthographic");
    };
    assert!(matches!(
        ortho.scaling_mode,
        ScalingMode::AutoMin { min_width, min_height } if min_width == 64.0 && min_height == 40.0
    ));
    assert_eq!(tf.translation.truncate(), Vec2::new(32.0, -20.0));
    let (centre, _) = camera_fit(&scenario);
    assert_eq!(centre, Vec2::new(32.0, -20.0));
    for (w, h) in [(1920u32, 993u32), (1280, 720), (800, 900), (300, 200)] {
        ortho.update(w as f32, h as f32);
        let fit = Fit::new((w, h), &scenario).unwrap();
        let s = (w as f32 / 64.0).min(h as f32 / 40.0);
        for spec in &scenario.stations {
            let grid = Vec2::new(f32::from(spec.x), f32::from(spec.y));
            let px = world_to_pixel(
                to_world(grid),
                centre,
                ortho.area,
                Vec2::new(w as f32, h as f32),
            );
            let back = fit.world((px.x.round() as i32, px.y.round() as i32));
            let tol = (MILLI as f32 / s) * 1.5;
            assert!(
                (back.x as f32 - grid.x * MILLI as f32).abs() <= tol
                    && (back.y as f32 - grid.y * MILLI as f32).abs() <= tol,
                "{w}x{h} station {grid:?} pixel {px:?} back {back:?}"
            );
        }
    }
}

fn lin(c: Srgba) -> [f32; 4] {
    LinearRgba::from(c).to_f32_array()
}

fn assert_within(
    g: &Geometry,
    range: std::ops::Range<usize>,
    centre: Vec2,
    half: Vec2,
    colour: Srgba,
) {
    for i in range {
        let p = g.positions[i];
        assert_eq!(p[2], 0.0);
        let d = Vec2::new(p[0], p[1]) - centre;
        assert!(
            d.x.abs() <= half.x + 1e-4 && d.y.abs() <= half.y + 1e-4,
            "vertex {i} {d:?} outside {half:?}"
        );
        assert_eq!(g.colors[i], lin(colour));
    }
}

#[test]
fn geometry_hand_values() {
    let shapes = [Shape::Circle, Shape::Triangle, Shape::Square, Shape::Star];
    let counts = [(4usize, 6usize), (4, 6), (4, 6), (4, 6)];
    let sc = Scene {
        tick: Some(Tick(1)),
        stations: shapes
            .iter()
            .enumerate()
            .map(|(i, &shape)| StationInst {
                id: StationId(i as u16),
                pos: Vec2::new(5.0 + 10.0 * i as f32, 7.0),
                shape,
                overflow: i as u32 % 2,
            })
            .collect(),
        passengers: shapes
            .iter()
            .enumerate()
            .map(|(i, &want)| PassengerInst {
                station: StationId(0),
                slot: i as u8,
                pos: slot_pos(Vec2::new(5.0, 7.0), i),
                want,
            })
            .collect(),
        badges: vec![salmong_game::render::BadgeInst {
            station: StationId(0),
            pos: slot_pos(Vec2::new(5.0, 7.0), QUEUE_CAP),
            extra: 3,
        }],
        riders: Vec::new(),
    };
    let g = station_geometry(&sc);
    let (mut v, mut ix) = (0, 0);
    for (i, s) in sc.stations.iter().enumerate() {
        let (nv, ni) = counts[i];
        assert_within(
            &g,
            v..v + nv,
            to_world(s.pos),
            Vec2::splat(STATION_HALF),
            station_colour(s.overflow),
        );
        v += nv;
        ix += ni;
    }
    assert_eq!((g.positions.len(), g.indices.len()), (v, ix));
    assert_eq!(station_colour(0).to_u8_array(), [255, 255, 255, 255]);
    assert_eq!(station_colour(1).to_u8_array(), [0xFF, 0xD6, 0x0A, 255]);
    let g = passenger_geometry(&sc);
    let (mut v, mut ix) = (0, 0);
    for (i, p) in sc.passengers.iter().enumerate() {
        let (nv, ni) = counts[i];
        assert_within(
            &g,
            v..v + nv,
            to_world(p.pos),
            Vec2::splat(PASSENGER_HALF),
            PASSENGER_COLOUR,
        );
        v += nv;
        ix += ni;
    }
    assert_within(
        &g,
        v..v + 4,
        to_world(sc.badges[0].pos),
        Vec2::splat(BADGE_HALF),
        BADGE_COLOUR,
    );
    assert_eq!((g.positions.len(), g.indices.len()), (v + 4, ix + 6));
    let bd = BusDraw {
        buses: vec![
            BusInst {
                id: BusId(0),
                pos: Vec2::new(10.0, 20.0),
                riders: 8,
            },
            BusInst {
                id: BusId(1),
                pos: Vec2::new(30.0, 5.0),
                riders: 2,
            },
        ],
    };
    let g = bus_geometry(&bd);
    assert_eq!(g.positions.len(), 4 + 6 * 4 + 4 + 2 * 4);
    assert_eq!(g.indices.len(), 6 * (1 + 6 + 1 + 2));
    assert_within(&g, 0..4, to_world(bd.buses[0].pos), BUS_HALF, BUS_COLOUR);
    for k in 0..6 {
        assert_within(
            &g,
            4 + 4 * k..8 + 4 * k,
            to_world(bd.buses[0].pos),
            BUS_HALF,
            RIDER_COLOUR,
        );
        let corner = g.positions[4 + 4 * k];
        let c =
            Vec2::new(corner[0], corner[1]) + Vec2::splat(RIDER_HALF) - to_world(bd.buses[0].pos);
        assert!(c.y > 0.0, "rider {k} not above the centre line");
    }
    let rd = RouteDraw {
        rev: Some(1),
        routes: vec![RouteInst {
            id: RouteId(2),
            points: vec![
                Vec2::new(5.0, 5.0),
                Vec2::new(15.0, 5.0),
                Vec2::new(15.0, 25.0),
            ],
        }],
    };
    let g = route_geometry(&rd);
    assert_eq!((g.positions.len(), g.indices.len()), (8, 12));
    assert_within(
        &g,
        0..4,
        to_world(Vec2::new(10.0, 5.0)),
        Vec2::new(5.0, ROUTE_HALF),
        route_colour(RouteId(2)),
    );
    assert_within(
        &g,
        4..8,
        to_world(Vec2::new(15.0, 15.0)),
        Vec2::new(ROUTE_HALF, 10.0),
        route_colour(RouteId(2)),
    );
    assert_eq!(
        route_colour(RouteId(0)).to_u8_array(),
        [0x1B, 0x9A, 0xAA, 255]
    );
}

fn layer_state(app: &mut App) -> Vec<(Layer, Option<usize>, Visibility)> {
    let handles: Vec<(Layer, Handle<Mesh>, Visibility)> = app
        .world_mut()
        .query::<(&Layer, &Mesh2d, &Visibility)>()
        .iter(app.world())
        .map(|(l, m, v)| (*l, m.0.clone(), *v))
        .collect();
    let meshes = app.world().resource::<Assets<Mesh>>();
    handles
        .into_iter()
        .map(|(l, h, v)| (l, meshes.get(&h).map(|m| m.count_vertices()), v))
        .collect()
}

#[test]
fn layer_meshes_follow_draw_data() {
    let mut app = demo_app();
    let mut shown = BTreeSet::new();
    while tick_of(&app) < 700 {
        frame(&mut app, step() / 2);
        let state = layer_state(&mut app);
        assert_eq!(state.len(), 4);
        let w = app.world();
        let sc = w.resource::<Scene>();
        for (layer, count, vis) in state {
            let g = match layer {
                Layer::Routes => route_geometry(w.resource::<RouteDraw>()),
                Layer::Stations => station_geometry(sc),
                Layer::Passengers => passenger_geometry(sc),
                Layer::Buses => bus_geometry(w.resource::<BusDraw>()),
            };
            if g.indices.is_empty() {
                assert_eq!(vis, Visibility::Hidden, "{layer:?}");
            } else {
                assert_eq!(vis, Visibility::Inherited, "{layer:?}");
                assert_eq!(count, Some(g.positions.len()), "{layer:?}");
                shown.insert(format!("{layer:?}"));
            }
        }
    }
    assert_eq!(shown.len(), 4, "{shown:?}");
}

fn demo_run(first: Duration, rest: Duration) -> App {
    let mut app = base_app();
    app.add_plugins((PackPlugin, DemoPlugin))
        .add_systems(PostStartup, demo::start);
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.update();
    assert!(app.world().contains_resource::<ScenarioPack>());
    assert!(app.world().contains_resource::<Demo>());
    frame(&mut app, first);
    while tick_of(&app) < END_TICK {
        frame(&mut app, rest);
    }
    app
}

fn assert_demo(app: &mut App) {
    let scenario = pack_scenario();
    let expected = hash_sequence(
        scenario.clone(),
        u64::from(scenario.seed),
        Balance::mvp(),
        &demo::body(),
        END_TICK + 1,
    );
    let lines = app.world().resource::<Demo>().lines.clone();
    assert_eq!(lines.len(), 7, "{lines:?}");
    for (i, l) in lines[..6].iter().enumerate() {
        let t = CHECK_EVERY * (i as u64 + 1);
        let prefix = format!(
            "salmong: replay tick={t} hash={:016x} frame=",
            expected[t as usize]
        );
        assert!(l.starts_with(&prefix), "{l} vs {prefix}");
    }
    assert_eq!(lines[6], "salmong: replay done tick=900");
    for _ in 0..20 {
        frame(app, step() * 3);
        assert_eq!(tick_of(app), END_TICK);
    }
    assert!(app.world().resource::<Clock>().paused());
    assert_eq!(
        app.world().resource::<Time<Fixed>>().overstep_fraction(),
        0.0
    );
    let track = app.world().resource::<BusTrack>();
    assert_eq!(
        app.world().resource::<BusDraw>().buses,
        buses(&track.at(0.0), &app.world().resource::<Scene>().riders)
    );
}

#[test]
fn demo_lines_match_hash_sequence_and_freeze() {
    let mut app = demo_run(step() / 2, step() / 2);
    assert_demo(&mut app);
    let frames = app.world().resource::<Demo>().frames;
    assert!(frames >= 1800, "{frames}");
    let mut app = demo_run(step(), step() * 3);
    assert_demo(&mut app);
    let mut app = demo_run(step() * 3, step() * 3);
    assert_demo(&mut app);
}

#[test]
fn demo_body_shows_every_kind() {
    let scenario = pack_scenario();
    let mut sim = Sim::new(scenario.clone(), u64::from(scenario.seed), Balance::mvp());
    let body = demo::body();
    let mut events = Vec::new();
    let mut riders = false;
    let mut bi = 0;
    for t in 0..END_TICK {
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        sim.step(&mut events);
        riders |= sim.view().buses.iter().any(|b| !b.riders.is_empty());
    }
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, SimEvent::CommandRejected))
    );
    let view = sim.view();
    assert_eq!(view.mode, SimMode::Running);
    assert_eq!(view.queues.len(), 4);
    assert!(view.route_count >= 2);
    assert_eq!(view.buses.len(), 2);
    assert!(view.queues.iter().any(|q| q.len() >= 3));
    assert!(riders);
}

#[test]
fn selects_demo_parses_query() {
    for q in ["?replay=demo", "replay=demo", "?a=1&replay=demo"] {
        assert!(demo::selects_demo(q), "{q}");
    }
    for q in ["", "?replay=demo2", "?xreplay=demo", "?replay="] {
        assert!(!demo::selects_demo(q), "{q}");
    }
}

#[test]
fn no_sim_res_draws_nothing() {
    let mut app = base_app();
    for _ in 0..5 {
        app.insert_resource(TimeUpdateStrategy::ManualDuration(step()));
        app.update();
    }
    assert_eq!(*app.world().resource::<Scene>(), Scene::default());
    assert_eq!(*app.world().resource::<RouteDraw>(), RouteDraw::default());
    assert_eq!(*app.world().resource::<BusDraw>(), BusDraw::default());
    let state = layer_state(&mut app);
    assert_eq!(state.len(), 4);
    assert!(state.iter().all(|(_, _, v)| *v == Visibility::Hidden));
}

fn polar(n: usize, k: usize, radius: f32) -> Vec2 {
    let a = std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * k as f32 / n as f32;
    Vec2::new(a.cos(), a.sin()) * radius
}

fn inside_polygon(p: Vec2, poly: &[Vec2]) -> bool {
    let mut inside = false;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
            inside = !inside;
        }
    }
    inside
}

fn m31_inside(shape: Shape, p: Vec2) -> bool {
    match shape {
        Shape::Circle => p.length() <= 1.0,
        Shape::Triangle => {
            let pts: Vec<Vec2> = (0..3).map(|k| polar(3, k, 1.0)).collect();
            inside_polygon(p, &pts)
        }
        Shape::Square => p.x.abs() <= 1.0 && p.y.abs() <= 1.0,
        Shape::Star => {
            let pts: Vec<Vec2> = (0..10)
                .map(|k| polar(10, k, if k % 2 == 0 { 1.0 } else { 0.45 }))
                .collect();
            inside_polygon(p, &pts)
        }
    }
}

#[test]
fn shape_distance_matches_m31_polygons() {
    let shapes = [Shape::Circle, Shape::Triangle, Shape::Square, Shape::Star];
    assert_eq!(shapes.map(shape_code), [0, 1, 2, 3]);
    for shape in shapes {
        let mut checked = 0;
        let mut inside = 0;
        for i in 0..41 {
            for j in 0..41 {
                let p = Vec2::new(-1.0 + i as f32 / 20.0, -1.0 + j as f32 / 20.0);
                let d = shape_distance(shape_code(shape), p);
                if d.abs() < 0.02 {
                    continue;
                }
                checked += 1;
                let want = m31_inside(shape, p);
                inside += usize::from(want);
                assert_eq!(d < 0.0, want, "{shape:?} at {p}: d={d}");
            }
        }
        assert!(checked > 1500, "{shape:?} checked {checked}");
        assert!(inside > 100, "{shape:?} inside {inside}");
    }
}

#[test]
fn hit_reads_shape_and_colour() {
    let shapes = [Shape::Circle, Shape::Triangle, Shape::Square, Shape::Star];
    let sc = Scene {
        stations: shapes
            .iter()
            .enumerate()
            .map(|(i, &shape)| StationInst {
                id: StationId(i as u16),
                pos: Vec2::new(4.0 * i as f32, 3.0),
                shape,
                overflow: 0,
            })
            .collect(),
        ..default()
    };
    let g = station_geometry(&sc);
    assert_eq!(g.uvs.len(), g.positions.len());
    let white = Some(lin(station_colour(0)));
    let star_notch = polar(10, 1, 0.55);
    let star_tip = polar(10, 0, 0.9);
    for (i, &shape) in shapes.iter().enumerate() {
        let c = to_world(Vec2::new(4.0 * i as f32, 3.0));
        let at = |l: Vec2| hit(&g, c + l * STATION_HALF);
        assert_eq!(at(Vec2::ZERO), white, "{shape:?} centre");
        assert_eq!(at(Vec2::new(0.0, 0.85)), white, "{shape:?} top");
        let corner = at(Vec2::new(0.85, 0.85));
        let base = at(Vec2::new(0.0, -0.75));
        let side = at(Vec2::new(0.9, 0.0));
        let notch = at(star_notch);
        let tip = at(star_tip);
        match shape {
            Shape::Circle => {
                assert_eq!(corner, None);
                assert_eq!(base, white);
                assert_eq!(side, white);
                assert_eq!(notch, white);
            }
            Shape::Triangle => {
                assert_eq!(corner, None);
                assert_eq!(base, None);
                assert_eq!(side, None);
            }
            Shape::Square => {
                assert_eq!(corner, white);
                assert_eq!(base, white);
                assert_eq!(side, white);
            }
            Shape::Star => {
                assert_eq!(corner, None);
                assert_eq!(notch, None);
                assert_eq!(tip, white);
            }
        }
        assert_eq!(hit(&g, c + Vec2::splat(1.2 * STATION_HALF)), None);
    }
    let bus = bus_geometry(&BusDraw {
        buses: vec![BusInst {
            id: BusId(0),
            pos: Vec2::new(5.0, 5.0),
            riders: 0,
        }],
    });
    let bc = to_world(Vec2::new(5.0, 5.0));
    assert_eq!(hit(&bus, bc + BUS_HALF * 0.95), Some(lin(BUS_COLOUR)));
    assert_eq!(hit(&bus, bc + BUS_HALF * 1.05), None);
}

#[test]
fn layers_share_one_shape_material() {
    let mut app = demo_app();
    while tick_of(&app) < 700 {
        frame(&mut app, step() / 2);
    }
    let state = layer_state(&mut app);
    assert!(
        state.iter().all(|(_, _, v)| *v == Visibility::Inherited),
        "{state:?}"
    );
    let mesh_count = app.world_mut().query::<&Mesh2d>().iter(app.world()).count();
    assert_eq!(mesh_count, 4);
    let layers: Vec<(Layer, Handle<Mesh>, Handle<ShapeMaterial>)> = app
        .world_mut()
        .query::<(&Layer, &Mesh2d, &MeshMaterial2d<ShapeMaterial>)>()
        .iter(app.world())
        .map(|(l, m, mat)| (*l, m.0.clone(), mat.0.clone()))
        .collect();
    assert_eq!(layers.len(), 4);
    let kinds: BTreeSet<String> = layers.iter().map(|(l, _, _)| format!("{l:?}")).collect();
    assert_eq!(kinds.len(), 4);
    let first = layers[0].2.id();
    assert!(layers.iter().all(|(_, _, mat)| mat.id() == first));
    assert_eq!(app.world().resource::<Assets<ShapeMaterial>>().len(), 1);
    assert!(app.world().entities().len() < 200);
    let meshes = app.world().resource::<Assets<Mesh>>();
    for (layer, handle, _) in &layers {
        let mesh = meshes.get(handle).expect("visible layer has a mesh");
        let len = |a| mesh.attribute(a).map(|v| v.len());
        let n = len(Mesh::ATTRIBUTE_POSITION);
        assert!(n.is_some_and(|n| n > 0), "{layer:?}");
        assert_eq!(len(Mesh::ATTRIBUTE_UV_0), n, "{layer:?}");
        assert_eq!(len(Mesh::ATTRIBUTE_COLOR), n, "{layer:?}");
    }
}
