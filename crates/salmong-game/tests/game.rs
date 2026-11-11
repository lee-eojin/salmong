#![cfg(not(target_arch = "wasm32"))]

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseButton, MouseButtonInput};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{CursorMoved, PrimaryWindow, WindowEvent, WindowOccluded, WindowResolution};
use salmong_core::balance::Balance;
use salmong_core::ids::{BusId, RouteId, StationId, Tick};
use salmong_core::replay::hash_sequence;
use salmong_core::scenario::Scenario;
use salmong_core::score::FinalScore;
use salmong_core::sim::{Command, Sim};
use salmong_game::best::{BestPlugin, BestRes, BestStore};
use salmong_game::bus::{BusPlugin, BusTrack};
use salmong_game::clock::{Clock, ClockCommand, ClockPlugin, Speed};
use salmong_game::demo;
use salmong_game::game::{
    Boot, GameLog, GamePlugin, Screen, body, expect, hud, key_command, over_prefix, pick,
    ready_line,
};
use salmong_game::input::{CommandLog, InputPlugin, PendingCommands, PointerRes, SimRes};
use salmong_game::pack::PackPlugin;
use salmong_game::render::{
    BusDraw, GHOST_HALF, Geometry, HudDraw, Layer, OverInst, PendingDraw, RenderPlugin, RouteDraw,
    RouteInst, Scene, ShapeMaterial, bus_geometry, ghost_geometry, hit, hud_geometry,
    pending_lines, route_geometry,
};
use salmong_game::sim::{ReplayFeed, SimPlugin};
use salmong_pack::parse_pack;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

const OVER_TICK: u64 = 2894;
const OVER_SCORE: u64 = 23;
const OVER_HASH: u64 = 0xb07dc97034555bfc;
const IDLE_TICK: u64 = 2894;
const IDLE_HASH: u64 = 0x8cc29a1392a381b1;

const PACK: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");
const STATION_PX: [(f32, f32); 3] = [(1041.0, 864.0), (447.0, 513.0), (1446.0, 891.0)];
const EMPTY_PX: (f32, f32) = (150.0, 54.0);

static NEXT: AtomicU32 = AtomicU32::new(0);

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("salmong-game-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }

    fn file(&self) -> PathBuf {
        self.0.join("best")
    }

    fn read(&self) -> Option<String> {
        fs::read_to_string(self.file()).ok()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Game {
    app: App,
    w: Entity,
    _dir: Dir,
}

fn scenario() -> Scenario {
    parse_pack(PACK).expect("pack").scenarios[0].clone()
}

fn fresh_hash() -> u64 {
    let sc = scenario();
    Sim::new(sc.clone(), u64::from(sc.seed), Balance::mvp()).state_hash()
}

fn step() -> Duration {
    Duration::from_nanos(33_333_333)
}

fn game_with(boot: Boot, dir: Dir) -> Game {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(StatesPlugin)
        .add_message::<WindowEvent>()
        .add_plugins(AssetPlugin::default())
        .init_asset::<Mesh>();
    let w = app
        .world_mut()
        .spawn((
            Window {
                resolution: WindowResolution::new(1920, 1080),
                ..default()
            },
            PrimaryWindow,
        ))
        .id();
    app.world_mut().spawn(Camera2d);
    app.insert_resource(BestStore::at(dir.file()));
    app.insert_resource(boot);
    app.add_plugins((
        InputPlugin,
        SimPlugin,
        BusPlugin,
        ClockPlugin,
        RenderPlugin,
        PackPlugin,
        BestPlugin,
        GamePlugin,
    ));
    let mut g = Game { app, w, _dir: dir };
    frame(&mut g, Duration::ZERO, vec![]);
    g
}

fn game(boot: Boot) -> Game {
    game_with(boot, Dir::new())
}

fn frame(g: &mut Game, d: Duration, events: Vec<WindowEvent>) {
    for e in events {
        g.app.world_mut().write_message(e);
    }
    g.app.insert_resource(TimeUpdateStrategy::ManualDuration(d));
    g.app.update();
}

fn frames(g: &mut Game, n: usize, d: Duration) {
    for _ in 0..n {
        frame(g, d, vec![]);
    }
}

fn tick_of(g: &Game) -> u64 {
    g.app.world().resource::<SimRes>().sim.view().tick.0
}

fn hash_of(g: &Game) -> u64 {
    g.app.world().resource::<SimRes>().sim.state_hash()
}

fn screen(g: &Game) -> Screen {
    *g.app.world().resource::<State<Screen>>().get()
}

fn lines(g: &Game) -> Vec<String> {
    g.app.world().resource::<GameLog>().lines.clone()
}

fn log(g: &Game) -> Vec<(Tick, Command)> {
    g.app.world().resource::<CommandLog>().0.clone()
}

fn clock(g: &Game) -> Clock {
    *g.app.world().resource::<Clock>()
}

fn hud_of(g: &Game) -> HudDraw {
    g.app.world().resource::<HudDraw>().clone()
}

fn moved(w: Entity, px: (f32, f32)) -> WindowEvent {
    WindowEvent::CursorMoved(CursorMoved {
        window: w,
        position: Vec2::from(px),
        delta: None,
    })
}

fn mouse(w: Entity, button: MouseButton, state: ButtonState) -> WindowEvent {
    WindowEvent::MouseButtonInput(MouseButtonInput {
        button,
        state,
        window: w,
    })
}

fn left(w: Entity, state: ButtonState) -> WindowEvent {
    mouse(w, MouseButton::Left, state)
}

fn key_ev(w: Entity, code: KeyCode, state: ButtonState, repeat: bool) -> WindowEvent {
    WindowEvent::KeyboardInput(KeyboardInput {
        key_code: code,
        logical_key: Key::Character("k".into()),
        state,
        text: None,
        repeat,
        window: w,
    })
}

fn key(w: Entity, code: KeyCode) -> WindowEvent {
    key_ev(w, code, ButtonState::Pressed, false)
}

fn occluded(w: Entity, occluded: bool) -> WindowEvent {
    WindowEvent::WindowOccluded(WindowOccluded {
        window: w,
        occluded,
    })
}

fn add(stops: &[u16]) -> Command {
    Command::AddRoute {
        stops: stops.iter().map(|&s| StationId(s)).collect(),
    }
}

fn strip_best(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|l| match l.find(" best=") {
            Some(i) if l.starts_with("salmong: game over ") => l[..i].to_string(),
            _ => l.clone(),
        })
        .collect()
}

fn over_line(g: &Game) -> String {
    lines(g)
        .into_iter()
        .rev()
        .find(|l| l.starts_with("salmong: game over "))
        .expect("over line")
}

fn run_to_over(g: &mut Game, rate: &[Duration]) {
    let mut i = 0;
    while screen(g) != Screen::GameOver {
        frame(g, rate[i % rate.len()], vec![]);
        i += 1;
        assert!(i < 40_000, "no game over, tick {}", tick_of(g));
    }
}

fn scripted_lines(boot: Boot) -> Vec<String> {
    let sc = scenario();
    let mut want = vec![ready_line(sc.id, u64::from(sc.seed), boot)];
    want.extend(expect(&body()));
    want
}

fn plain(body: &[(Tick, Command)]) -> (FinalScore, u64) {
    let sc = scenario();
    let mut sim = Sim::new(sc.clone(), u64::from(sc.seed), Balance::mvp());
    let mut events = Vec::new();
    let mut bi = 0;
    while sim.final_score().is_none() {
        let t = sim.view().tick.0;
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        sim.step(&mut events);
        assert!(t < 100_000);
    }
    (sim.final_score().expect("over"), sim.state_hash())
}

#[test]
fn scripted_bodies_end_at_the_pinned_tick_score_and_hash() {
    let (run, hash) = plain(&body());
    assert_eq!(
        (run.ticks, run.score, hash),
        (OVER_TICK, OVER_SCORE, OVER_HASH)
    );
    let (run, hash) = plain(&[]);
    assert_eq!((run.ticks, run.score, hash), (IDLE_TICK, 0, IDLE_HASH));
    let sc = scenario();
    let seq = hash_sequence(
        sc.clone(),
        u64::from(sc.seed),
        Balance::mvp(),
        &body(),
        OVER_TICK,
    );
    assert_eq!(seq[OVER_TICK as usize], OVER_HASH);
    let seq = hash_sequence(
        sc.clone(),
        u64::from(sc.seed),
        Balance::mvp(),
        &[],
        IDLE_TICK,
    );
    assert_eq!(seq[IDLE_TICK as usize], IDLE_HASH);
    const { assert!(OVER_SCORE > 0) };
    assert_eq!(
        expect(&body()).last(),
        Some(&over_prefix(OVER_TICK, OVER_SCORE, OVER_HASH))
    );
    assert_eq!(
        expect(&[]).last(),
        Some(&over_prefix(IDLE_TICK, 0, IDLE_HASH))
    );
}

#[test]
fn replay_boot_prints_the_headless_lines_at_every_rate() {
    let rates: [Vec<Duration>; 4] = [
        vec![step() / 2],
        vec![step()],
        vec![step() * 3],
        vec![
            step() * 2 / 5,
            step() * 13 / 10,
            step() / 20,
            step() * 26 / 10,
            step() * 9 / 10,
        ],
    ];
    for rate in rates {
        let mut g = game(Boot::Replay);
        assert_eq!(screen(&g), Screen::Playing);
        run_to_over(&mut g, &rate);
        frames(&mut g, 20, rate[0]);
        assert_eq!(screen(&g), Screen::GameOver);
        assert_eq!(tick_of(&g), OVER_TICK);
        assert_eq!(
            strip_best(&lines(&g)),
            scripted_lines(Boot::Replay),
            "{rate:?}"
        );
        assert!(over_line(&g).ends_with(" best=none"));
        assert_eq!(log(&g), body());
    }
}

#[test]
fn replay_does_not_write_the_best() {
    let dir = Dir::new();
    let mut g = game_with(Boot::Replay, dir);
    run_to_over(&mut g, &[step()]);
    frames(&mut g, 5, step());
    assert_eq!(g.app.world().resource::<BestRes>().0, None);
    assert!(over_line(&g).ends_with(" best=none"));
    assert_eq!(g._dir.read(), None);

    let dir = Dir::new();
    fs::write(dir.file(), "salmong.best.1 1 5").expect("seed");
    let mut g = game_with(Boot::Replay, dir);
    run_to_over(&mut g, &[step()]);
    frames(&mut g, 5, step());
    let held = FinalScore { score: 1, ticks: 5 };
    assert_eq!(g.app.world().resource::<BestRes>().0, Some(held));
    assert!(over_line(&g).ends_with(" best=1,5"));
    assert_eq!(g._dir.read().as_deref(), Some("salmong.best.1 1 5"));
    assert_eq!(
        hud_of(&g).over,
        Some(OverInst {
            score: OVER_SCORE,
            best: Some(1)
        })
    );
}

fn noise(w: Entity) -> Vec<WindowEvent> {
    vec![
        moved(w, STATION_PX[2]),
        left(w, ButtonState::Pressed),
        moved(w, STATION_PX[0]),
        left(w, ButtonState::Released),
        key(w, KeyCode::Space),
        key(w, KeyCode::Digit2),
    ]
}

#[test]
fn replay_ignores_pointer_and_keys() {
    let mut g = game(Boot::Replay);
    let mut i = 0;
    while screen(&g) != Screen::GameOver {
        let ev = if i % 50 == 0 { noise(g.w) } else { vec![] };
        frame(&mut g, step() / 2, ev);
        assert!(g.app.world().resource::<PendingDraw>().lines.is_empty());
        i += 1;
        assert!(i < 40_000);
    }
    frames(&mut g, 20, step() / 2);
    assert_eq!(strip_best(&lines(&g)), scripted_lines(Boot::Replay));
    assert_eq!(log(&g), body());
    assert_eq!(clock(&g), Clock::default());

    let mut g = game(Boot::Play);
    let w = g.w;
    frame(&mut g, step() / 2, noise(w));
    frames(&mut g, 4, step() / 2);
    assert_eq!(screen(&g), Screen::Playing);
    assert!(clock(&g).paused());
    assert_eq!(
        g.app.world().resource::<PendingCommands>().0,
        vec![add(&[2, 0])]
    );
    frame(&mut g, step() / 2, vec![key(w, KeyCode::Space)]);
    frames(&mut g, 6, step() / 2);
    assert!(!clock(&g).paused());
    assert_eq!(log(&g), vec![(Tick(0), add(&[2, 0]))]);
}

#[test]
fn menu_waits_without_clock_pause_until_the_first_left_press() {
    let mut g = game(Boot::Play);
    let w = g.w;
    assert_eq!(screen(&g), Screen::Menu);
    assert_eq!(g.app.world().resource::<Scene>().stations.len(), 3);
    for i in 0..120 {
        let ev = match i {
            10 => vec![mouse(w, MouseButton::Right, ButtonState::Pressed)],
            11 => vec![mouse(w, MouseButton::Right, ButtonState::Released)],
            20 => vec![key(w, KeyCode::KeyA)],
            _ => vec![moved(w, (100.0 + i as f32 * 7.0, 300.0))],
        };
        frame(&mut g, step() / 2, ev);
    }
    assert_eq!(screen(&g), Screen::Menu);
    assert_eq!(tick_of(&g), 0);
    assert_eq!(hash_of(&g), fresh_hash());
    assert_eq!(clock(&g), Clock::default());
    assert!(!g.app.world().resource::<Time<Virtual>>().is_paused());
    assert!(hud_geometry(&hud_of(&g)).indices.is_empty());
    assert!(
        !lines(&g)
            .iter()
            .any(|l| l.contains("start") || l.contains("clock"))
    );
    frame(
        &mut g,
        step() / 2,
        vec![moved(w, EMPTY_PX), left(w, ButtonState::Pressed)],
    );
    frame(&mut g, step() / 2, vec![left(w, ButtonState::Released)]);
    assert_eq!(screen(&g), Screen::Playing);
    assert!(lines(&g).contains(&"salmong: game start".to_string()));
    let before = tick_of(&g);
    frames(&mut g, 60, step() / 2);
    let ran = tick_of(&g) - before;
    assert!((29..=31).contains(&ran), "{ran}");
    assert!(log(&g).is_empty());
}

#[test]
fn first_press_also_begins_the_drag() {
    let mut g = game(Boot::Play);
    let w = g.w;
    frame(
        &mut g,
        step() / 2,
        vec![moved(w, STATION_PX[0]), left(w, ButtonState::Pressed)],
    );
    frame(&mut g, step() / 2, vec![moved(w, STATION_PX[1])]);
    frame(&mut g, step() / 2, vec![left(w, ButtonState::Released)]);
    frames(&mut g, 10, step() / 2);
    assert_eq!(screen(&g), Screen::Playing);
    let cmds: Vec<Command> = log(&g).into_iter().map(|(_, c)| c).collect();
    assert_eq!(cmds, vec![add(&[0, 1])]);
}

fn paused_drag(g: &mut Game) {
    let w = g.w;
    frame(g, step() / 2, vec![key(w, KeyCode::Space)]);
    frame(
        g,
        step() / 2,
        vec![moved(w, STATION_PX[0]), left(w, ButtonState::Pressed)],
    );
    frame(g, step() / 2, vec![moved(w, STATION_PX[1])]);
    frame(g, step() / 2, vec![left(w, ButtonState::Released)]);
    frames(g, 30, step() / 2);
}

fn play_scripted(g: &mut Game) {
    let w = g.w;
    paused_drag(g);
    frame(g, step() / 2, vec![key(w, KeyCode::Space)]);
    frames(g, 6, step() / 2);
    frame(g, step() / 2, vec![key(w, KeyCode::Digit2)]);
    frames(g, 20, step() / 2);
    frame(g, step() / 2, vec![occluded(w, true)]);
    frames(g, 30, step() / 2);
    frame(g, step() / 2, vec![occluded(w, false)]);
    frame(g, Duration::from_secs(10), vec![]);
    run_to_over(g, &[step() / 2]);
    frames(g, 3, step() / 2);
}

#[test]
fn paused_drag_in_menu_plays_the_scripted_game() {
    let mut g = game(Boot::Play);
    let w = g.w;
    paused_drag(&mut g);
    assert_eq!(tick_of(&g), 0);
    assert_eq!(g.app.world().resource::<PendingCommands>().0.len(), 1);
    assert_eq!(
        g.app.world().resource::<PendingDraw>().lines,
        vec![vec![Vec2::new(35.0, 32.0), Vec2::new(13.0, 19.0)]]
    );
    assert!(g.app.world().resource::<RouteDraw>().routes.is_empty());
    frame(&mut g, step() / 2, vec![key(w, KeyCode::Space)]);
    frames(&mut g, 6, step() / 2);
    assert_eq!(log(&g), body());
    assert!(g.app.world().resource::<PendingDraw>().lines.is_empty());
    frame(&mut g, step() / 2, vec![key(w, KeyCode::Digit2)]);
    frames(&mut g, 20, step() / 2);
    frame(&mut g, step() / 2, vec![occluded(w, true)]);
    let hidden_at = tick_of(&g);
    frames(&mut g, 30, step() / 2);
    assert_eq!(tick_of(&g), hidden_at);
    frame(&mut g, step() / 2, vec![occluded(w, false)]);
    frame(&mut g, Duration::from_secs(10), vec![]);
    run_to_over(&mut g, &[step() / 2]);
    frames(&mut g, 3, step() / 2);
    let all = lines(&g);
    let ticks: Vec<String> = strip_best(&all)
        .into_iter()
        .filter(|l| l.starts_with("salmong: game tick=") || l.starts_with("salmong: game over "))
        .collect();
    assert_eq!(ticks, expect(&body()));
    let clocks: Vec<&String> = all.iter().filter(|l| l.contains("game clock")).collect();
    assert_eq!(clocks.len(), 3, "{clocks:?}");
    assert_eq!(clocks[0], "salmong: game clock paused=1 speed=1 tick=0");
    assert_eq!(clocks[1], "salmong: game clock paused=0 speed=1 tick=0");
    assert!(clocks[2].starts_with("salmong: game clock paused=0 speed=2 tick="));
    let run = FinalScore {
        score: OVER_SCORE,
        ticks: OVER_TICK,
    };
    assert_eq!(g.app.world().resource::<BestRes>().0, Some(run));
    assert!(over_line(&g).ends_with(&format!(" best={OVER_SCORE},{OVER_TICK}")));
    assert_eq!(
        g._dir.read(),
        Some(format!("salmong.best.1 {OVER_SCORE} {OVER_TICK}"))
    );
}

#[test]
fn game_over_shows_best_then_restart_replays_scenario_zero() {
    let mut g = game(Boot::Play);
    let w = g.w;
    play_scripted(&mut g);
    assert!(!g.app.world().resource::<BusDraw>().buses.is_empty());
    assert_eq!(
        hud_of(&g).over,
        Some(OverInst {
            score: OVER_SCORE,
            best: Some(OVER_SCORE)
        })
    );
    frames(&mut g, 10, step());
    frame(
        &mut g,
        step(),
        vec![moved(w, EMPTY_PX), left(w, ButtonState::Pressed)],
    );
    frame(&mut g, step(), vec![left(w, ButtonState::Released)]);
    frames(&mut g, 2, step());
    assert_eq!(screen(&g), Screen::GameOver);
    frames(&mut g, 20, step());
    let ticks_before = g.app.world().resource::<Time<Fixed>>().elapsed();
    frame(
        &mut g,
        Duration::ZERO,
        vec![moved(w, STATION_PX[0]), left(w, ButtonState::Pressed)],
    );
    assert_eq!(
        g.app.world().resource::<Time<Fixed>>().elapsed(),
        ticks_before
    );
    frame(&mut g, Duration::ZERO, vec![moved(w, STATION_PX[1])]);
    assert_eq!(screen(&g), Screen::Menu);
    frame(&mut g, Duration::ZERO, vec![left(w, ButtonState::Released)]);
    frames(&mut g, 2, step());
    assert_eq!(screen(&g), Screen::Menu);
    assert_eq!(tick_of(&g), 0);
    assert_eq!(hash_of(&g), fresh_hash());
    assert!(log(&g).is_empty());
    assert!(g.app.world().resource::<PendingCommands>().0.is_empty());
    assert!(g.app.world().resource::<PointerRes>().0.trail().is_empty());
    assert!(g.app.world().resource::<BusDraw>().buses.is_empty());
    assert!(g.app.world().resource::<RouteDraw>().routes.is_empty());
    assert_eq!(hud_of(&g).over, None);
    assert_eq!(clock(&g), Clock::default());
    let sc = scenario();
    let ready = ready_line(sc.id, u64::from(sc.seed), Boot::Play);
    assert_eq!(lines(&g).iter().filter(|l| **l == ready).count(), 2);
    frames(&mut g, 60, step() / 2);
    assert_eq!(screen(&g), Screen::Menu);
    assert_eq!(tick_of(&g), 0);
    let starts = lines(&g)
        .iter()
        .filter(|l| *l == "salmong: game start")
        .count();
    assert_eq!(starts, 1);
    frame(
        &mut g,
        step() / 2,
        vec![moved(w, EMPTY_PX), left(w, ButtonState::Pressed)],
    );
    frame(
        &mut g,
        step() / 2,
        vec![left(w, ButtonState::Released), key(w, KeyCode::Digit2)],
    );
    run_to_over(&mut g, &[step() / 2]);
    frames(&mut g, 3, step() / 2);
    let idle_last = expect(&[]).pop().expect("idle over");
    assert_eq!(
        over_line(&g),
        format!("{idle_last} best={OVER_SCORE},{OVER_TICK}")
    );
    assert_eq!(
        hud_of(&g).over,
        Some(OverInst {
            score: 0,
            best: Some(OVER_SCORE)
        })
    );
}

#[test]
fn keys_map_to_clock_commands() {
    let w = Entity::PLACEHOLDER;
    let p = ButtonState::Pressed;
    let table = [
        (KeyCode::Space, p, false, false, Some(ClockCommand::Pause)),
        (KeyCode::Space, p, false, true, Some(ClockCommand::Resume)),
        (
            KeyCode::Digit1,
            p,
            false,
            false,
            Some(ClockCommand::SetSpeed(Speed::X1)),
        ),
        (
            KeyCode::Digit2,
            p,
            false,
            true,
            Some(ClockCommand::SetSpeed(Speed::X2)),
        ),
        (KeyCode::Space, ButtonState::Released, false, false, None),
        (KeyCode::Space, p, true, false, None),
        (KeyCode::Digit2, p, true, false, None),
        (KeyCode::Escape, p, false, false, None),
        (KeyCode::Tab, p, false, false, None),
        (KeyCode::KeyA, p, false, false, None),
        (KeyCode::Numpad2, p, false, false, None),
    ];
    for (code, state, repeat, paused, want) in table {
        assert_eq!(
            key_command(&key_ev(w, code, state, repeat), paused),
            want,
            "{code:?} {state:?} {repeat} {paused}"
        );
    }
    assert_eq!(key_command(&moved(w, (1.0, 1.0)), false), None);
    let mut g = game(Boot::Play);
    let w = g.w;
    frame(
        &mut g,
        step() / 2,
        vec![key(w, KeyCode::Space), key(w, KeyCode::Space)],
    );
    frames(&mut g, 3, step() / 2);
    assert!(!clock(&g).paused());
    frame(&mut g, step() / 2, vec![key(w, KeyCode::Space)]);
    frames(&mut g, 2, step() / 2);
    assert!(clock(&g).paused());
}

fn clock_with(cmds: &[ClockCommand]) -> Clock {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<WindowEvent>()
        .add_plugins(ClockPlugin);
    for c in cmds {
        app.world_mut().write_message(*c);
    }
    app.update();
    *app.world().resource::<Clock>()
}

#[test]
fn hud_hand_table() {
    let grid = Vec2::new(64.0, 40.0);
    let both = clock_with(&[ClockCommand::Pause, ClockCommand::SetSpeed(Speed::X2)]);
    assert!(both.paused() && both.speed() == Speed::X2);
    let run = Some(FinalScore {
        score: 7,
        ticks: 100,
    });
    let best = Some(FinalScore {
        score: 9,
        ticks: 50,
    });
    assert_eq!(
        hud(Screen::Loading, &both, run, best, grid),
        HudDraw::default()
    );
    let on = HudDraw {
        grid,
        paused: true,
        fast: true,
        over: None,
    };
    assert_eq!(hud(Screen::Menu, &both, run, best, grid), on);
    assert_eq!(hud(Screen::Playing, &both, run, best, grid), on);
    assert_eq!(
        hud(Screen::GameOver, &both, run, best, grid),
        HudDraw {
            over: Some(OverInst {
                score: 7,
                best: Some(9)
            }),
            ..on.clone()
        }
    );
    assert_eq!(
        hud(Screen::GameOver, &both, run, None, grid).over,
        Some(OverInst {
            score: 7,
            best: None
        })
    );
    assert_eq!(hud(Screen::GameOver, &both, None, best, grid).over, None);
    assert_eq!(
        hud(Screen::Menu, &Clock::default(), None, None, grid),
        HudDraw { grid, ..default() }
    );
}

const SIGN: [u8; 3] = [0x14, 0x21, 0x3D];
const WHITE: [u8; 3] = [0xFF, 0xFF, 0xFF];
const STAR: [u8; 3] = [0xFF, 0xD6, 0x0A];
const LIT: [&str; 10] = [
    "abcdef", "bc", "abdeg", "abcdg", "bcfg", "acdfg", "acdefg", "abc", "abcdefg", "abcdfg",
];
const SEG: [(f32, f32); 7] = [
    (0.0, -1.4),
    (0.7, -0.7),
    (0.7, 0.7),
    (0.0, 1.4),
    (-0.7, 0.7),
    (-0.7, -0.7),
    (0.0, 0.0),
];

fn colour_at(g: &Geometry, grid: (f32, f32)) -> Option<[u8; 3]> {
    hit(g, Vec2::new(grid.0, -grid.1)).map(|c| {
        let a = Srgba::from(LinearRgba::new(c[0], c[1], c[2], c[3])).to_u8_array();
        [a[0], a[1], a[2]]
    })
}

fn read_row(g: &Geometry, y: f32) -> String {
    let mut out = String::new();
    for k in 0..12 {
        let cx = 28.0 + 2.2 * k as f32;
        let mut lit = String::new();
        for (i, (dx, dy)) in SEG.iter().enumerate() {
            match colour_at(g, (cx + dx, y + dy)) {
                Some(WHITE) => lit.push((b'a' + i as u8) as char),
                Some(SIGN) | None => {}
                other => panic!("segment {k} {i} at row {y}: {other:?}"),
            }
        }
        if lit.is_empty() {
            break;
        }
        let d = LIT.iter().position(|l| *l == lit).expect("known digit");
        out.push((b'0' + d as u8) as char);
    }
    out
}

fn over_hud(score: u64, best: Option<u64>) -> Geometry {
    hud_geometry(&HudDraw {
        grid: Vec2::new(64.0, 40.0),
        paused: false,
        fast: false,
        over: Some(OverInst { score, best }),
    })
}

#[test]
fn hud_and_ghost_geometry_hand_values() {
    let grid = Vec2::new(64.0, 40.0);
    let g = hud_geometry(&HudDraw {
        grid,
        paused: true,
        fast: true,
        over: None,
    });
    assert_eq!(colour_at(&g, (60.5, 2.0)), Some(SIGN));
    assert_eq!(colour_at(&g, (61.5, 2.0)), Some(SIGN));
    assert_eq!(colour_at(&g, (60.5, 2.8)), Some(SIGN));
    assert_eq!(colour_at(&g, (61.0, 2.0)), None);
    for cx in [57.0, 58.4] {
        assert_eq!(colour_at(&g, (cx + 0.6, 2.0)), Some(SIGN), "{cx}");
        assert_eq!(colour_at(&g, (cx - 0.3, 2.7)), None, "{cx}");
    }
    assert!(
        hud_geometry(&HudDraw { grid, ..default() })
            .indices
            .is_empty()
    );
    for d in 0..10u64 {
        let g = over_hud(d, Some(d));
        assert_eq!(read_row(&g, 17.0), d.to_string());
        assert_eq!(read_row(&g, 23.0), d.to_string());
    }
    assert_eq!(read_row(&over_hud(1234567890, None), 17.0), "9999999");
    assert_eq!(read_row(&over_hud(1234567, None), 17.0), "1234567");
    assert_eq!(read_row(&over_hud(890, None), 17.0), "890");
    let g = over_hud(0, Some(407));
    assert_eq!(read_row(&g, 17.0), "0");
    assert_eq!(read_row(&g, 23.0), "407");
    assert_eq!(colour_at(&g, (24.0, 17.0)), Some(WHITE));
    assert_eq!(colour_at(&g, (24.0, 23.0)), Some(STAR));
    assert_eq!(colour_at(&g, (42.0, 20.0)), Some(SIGN));
    assert_eq!(colour_at(&g, (20.5, 13.5)), Some(SIGN));
    assert_eq!(colour_at(&g, (19.5, 20.0)), None);
    assert_eq!(read_row(&over_hud(5, None), 23.0), "");
    assert_eq!(GHOST_HALF, 0.07);
    let ghost = ghost_geometry(&PendingDraw {
        lines: vec![vec![Vec2::new(35.0, 32.0), Vec2::new(13.0, 19.0)]],
    });
    assert_eq!(ghost.indices.len(), 6);
    assert_eq!(ghost.positions.len(), 4);
    let a = Vec2::new(35.0, -32.0);
    let b = Vec2::new(13.0, -19.0);
    let dir = (b - a).normalize();
    for p in &ghost.positions {
        let v = Vec2::new(p[0], p[1]);
        let off = (v - a).perp_dot(dir).abs();
        assert!((off - 0.07).abs() < 1e-4, "{off}");
    }
    assert_eq!(colour_at(&ghost, (26.2, 26.8)), Some([0x11, 0x11, 0x11]));
    let n = dir.perp() * 0.1;
    assert_eq!(colour_at(&ghost, (26.2 + n.x, 26.8 - n.y)), None);
    let stations = scenario().stations;
    let routes = vec![RouteInst {
        id: RouteId(3),
        points: vec![Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0)],
    }];
    let pending = vec![
        add(&[0, 2]),
        Command::EditRoute {
            route: RouteId(0),
            stops: vec![StationId(1), StationId(0)],
        },
        Command::RemoveRoute { route: RouteId(3) },
        Command::RemoveRoute { route: RouteId(4) },
        add(&[0, 99]),
    ];
    assert_eq!(
        pending_lines(&pending, &routes, &stations),
        vec![
            vec![Vec2::new(35.0, 32.0), Vec2::new(50.0, 33.0)],
            vec![Vec2::new(13.0, 19.0), Vec2::new(35.0, 32.0)],
            vec![Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0)],
        ]
    );
}

fn demo_app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<WindowEvent>()
        .add_plugins(AssetPlugin::default())
        .init_asset::<Mesh>()
        .add_plugins((InputPlugin, SimPlugin, BusPlugin, ClockPlugin, RenderPlugin));
    app.world_mut().spawn(Camera2d);
    let sc = scenario();
    app.insert_resource(SimRes {
        sim: Sim::new(sc.clone(), u64::from(sc.seed), Balance::mvp()),
        scenario: sc,
    });
    app.insert_resource(ReplayFeed::new(demo::body()));
    app
}

fn demo_frame(app: &mut App, d: Duration, msg: Option<WindowEvent>, cmd: Option<ClockCommand>) {
    if let Some(m) = msg {
        app.world_mut().write_message(m);
    }
    if let Some(c) = cmd {
        app.world_mut().write_message(c);
    }
    app.insert_resource(TimeUpdateStrategy::ManualDuration(d));
    app.update();
}

fn drawn(app: &App) -> Vec<(BusId, Vec2)> {
    app.world()
        .resource::<BusDraw>()
        .buses
        .iter()
        .map(|b| (b.id, b.pos))
        .collect()
}

fn progress(track: &BusTrack, id: BusId, pos: Vec2) -> Option<f32> {
    let p = track.prev().iter().find(|(i, _)| *i == id)?.1;
    let c = track.cur().iter().find(|(i, _)| *i == id)?.1;
    let d = c - p;
    (d.length_squared() > 1e-6).then(|| (pos - p).dot(d) / d.length_squared())
}

fn moving(app: &App) -> bool {
    let t = app.world().resource::<BusTrack>();
    t.cur().iter().any(|(id, c)| {
        t.prev()
            .iter()
            .any(|(pid, p)| pid == id && (*c - *p).length() > 1e-3)
    })
}

fn raw(app: &App) -> f32 {
    app.world().resource::<Time<Fixed>>().overstep_fraction()
}

fn check_resume(
    halt: (Option<WindowEvent>, Option<ClockCommand>),
    go: (Option<WindowEvent>, Option<ClockCommand>),
) {
    let mut app = demo_app();
    let d = step() * 3 / 10;
    let mut n = 0;
    while !(tick_of_app(&app) > 40 && moving(&app) && raw(&app) > 0.2) {
        demo_frame(&mut app, d, None, None);
        n += 1;
        assert!(n < 2000);
    }
    demo_frame(&mut app, d, halt.0, halt.1);
    let paused = drawn(&app);
    let tick = tick_of_app(&app);
    for _ in 0..10 {
        demo_frame(&mut app, d, None, None);
        assert_eq!(drawn(&app), paused);
    }
    demo_frame(&mut app, d, go.0, go.1);
    assert_eq!(raw(&app), 0.0);
    assert_eq!(tick_of_app(&app), tick);
    assert_eq!(drawn(&app), paused);
    let start: Vec<(BusId, f32)> = paused
        .iter()
        .filter_map(|&(id, p)| progress(app.world().resource::<BusTrack>(), id, p).map(|f| (id, f)))
        .collect();
    assert!(!start.is_empty());
    let mut last = start;
    let step_small = step() / 10;
    while tick_of_app(&app) == tick {
        demo_frame(&mut app, step_small, None, None);
        if tick_of_app(&app) != tick {
            break;
        }
        let now = drawn(&app);
        for (id, f) in &mut last {
            let pos = now.iter().find(|(i, _)| i == id).expect("bus").1;
            let g = progress(app.world().resource::<BusTrack>(), *id, pos).expect("moving");
            assert!(g >= *f - 1e-5, "{g} < {f}");
            *f = g;
        }
    }
}

fn tick_of_app(app: &App) -> u64 {
    app.world().resource::<SimRes>().sim.view().tick.0
}

#[test]
fn resume_does_not_slide_buses_back() {
    check_resume(
        (None, Some(ClockCommand::Pause)),
        (None, Some(ClockCommand::Resume)),
    );
    let w = Entity::PLACEHOLDER;
    check_resume(
        (Some(occluded(w, true)), None),
        (Some(occluded(w, false)), None),
    );
}

fn layer_vertices(app: &mut App, layer: Layer) -> (Vec<[f32; 3]>, Visibility) {
    let found: Vec<(Handle<Mesh>, Visibility)> = app
        .world_mut()
        .query::<(&Layer, &Mesh2d, &Visibility)>()
        .iter(app.world())
        .filter(|(l, _, _)| **l == layer)
        .map(|(_, m, v)| (m.0.clone(), *v))
        .collect();
    assert_eq!(found.len(), 1);
    let (h, vis) = found.into_iter().next().expect("layer");
    let meshes = app.world().resource::<Assets<Mesh>>();
    let pos = meshes
        .get(&h)
        .and_then(|m| m.attribute(Mesh::ATTRIBUTE_POSITION))
        .and_then(|a| a.as_float3())
        .map(|a| a.to_vec())
        .unwrap_or_default();
    (pos, vis)
}

fn check_layers(g: &mut Game) {
    let app = &mut g.app;
    assert_eq!(
        app.world_mut().query::<&Mesh2d>().iter(app.world()).count(),
        4
    );
    assert_eq!(app.world().resource::<Assets<ShapeMaterial>>().len(), 1);
    let w = app.world();
    let mut buses = bus_geometry(w.resource::<BusDraw>()).positions;
    let hud = hud_geometry(w.resource::<HudDraw>());
    assert!(!hud.positions.is_empty());
    buses.extend(hud.positions);
    let mut routes = route_geometry(w.resource::<RouteDraw>()).positions;
    routes.extend(ghost_geometry(w.resource::<PendingDraw>()).positions);
    let (got, vis) = layer_vertices(app, Layer::Buses);
    assert_eq!(got, buses);
    assert_eq!(vis, Visibility::Inherited);
    let (got, vis) = layer_vertices(app, Layer::Routes);
    if routes.is_empty() {
        assert_eq!(vis, Visibility::Hidden);
    } else {
        assert_eq!(got, routes);
        assert_eq!(vis, Visibility::Inherited);
    }
}

#[test]
fn game_draws_inside_the_four_layer_meshes() {
    let mut g = game(Boot::Play);
    let w = g.w;
    frame(&mut g, step() / 2, vec![key(w, KeyCode::Space)]);
    frames(&mut g, 2, step() / 2);
    assert_eq!(screen(&g), Screen::Menu);
    assert!(g.app.world().resource::<BusDraw>().buses.is_empty());
    check_layers(&mut g);
    frame(
        &mut g,
        step() / 2,
        vec![moved(w, STATION_PX[0]), left(w, ButtonState::Pressed)],
    );
    frame(&mut g, step() / 2, vec![moved(w, STATION_PX[1])]);
    frame(&mut g, step() / 2, vec![left(w, ButtonState::Released)]);
    frames(&mut g, 2, step() / 2);
    assert!(!g.app.world().resource::<PendingDraw>().lines.is_empty());
    assert!(g.app.world().resource::<RouteDraw>().routes.is_empty());
    check_layers(&mut g);
    frame(&mut g, step() / 2, vec![key(w, KeyCode::Space)]);
    run_to_over(&mut g, &[step()]);
    frames(&mut g, 3, step());
    assert!(hud_of(&g).over.is_some());
    check_layers(&mut g);
}

#[test]
fn boot_pick_table() {
    assert_eq!(pick(false, false), Boot::Play);
    assert_eq!(pick(false, true), Boot::Replay);
    assert_eq!(pick(true, false), Boot::Off);
    assert_eq!(pick(true, true), Boot::Off);
    assert!(demo::selects("?replay=over", "over"));
    assert!(!demo::selects("?replay=over", "demo"));
}
