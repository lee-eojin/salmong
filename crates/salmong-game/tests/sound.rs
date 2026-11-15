#![cfg(not(target_arch = "wasm32"))]

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseButton, MouseButtonInput};
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{CursorMoved, PrimaryWindow, WindowEvent, WindowFocused, WindowResolution};
use salmong_core::balance::Balance;
use salmong_core::ids::{BusId, PassengerId, StationId, Tick};
use salmong_core::replay::hash_sequence;
use salmong_core::scenario::{Scenario, Shape};
use salmong_core::sim::{Command, SimEvent};
use salmong_game::best::{BestPlugin, BestStore};
use salmong_game::bus::BusPlugin;
use salmong_game::clock::ClockPlugin;
use salmong_game::game::{Boot, GameLog, GamePlugin, Screen, body, over_prefix};
use salmong_game::input::{CommandLog, InputPlugin, SimRes};
use salmong_game::pack::PackPlugin;
use salmong_game::render::{
    BusDraw, Geometry, HudDraw, Layer, RenderPlugin, ShapeMaterial, bus_geometry, hit, hud_geometry,
};
use salmong_game::sim::{SimEventMessage, SimPlugin, Ticks, step_sim};
use salmong_game::sound::{
    Cue, CueMessage, MuteStore, Muted, SoundPlugin, cue, gap, mute_key, mute_line, volume, wav,
};
use salmong_pack::parse_pack;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

const OVER_TICK: u64 = 2894;
const OVER_SCORE: u64 = 23;
const OVER_HASH: u64 = 0xb07d_c970_3455_5bfc;
const MUTE_TICKS: [u64; 3] = [100, 400, 900];

const PACK: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");
const STATION_PX: [(f32, f32); 2] = [(1041.0, 864.0), (447.0, 513.0)];
const SOUNDS: [(Cue, usize); 6] = [
    (Cue::Board, 1_102),
    (Cue::Deliver, 2_646),
    (Cue::Station, 6_615),
    (Cue::Overflow, 6_615),
    (Cue::Reject, 3_307),
    (Cue::Over, 19_845),
];

static NEXT: AtomicU32 = AtomicU32::new(0);

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("salmong-sound-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }

    fn muted(text: &str) -> Self {
        let dir = Self::new();
        fs::write(dir.mute(), text).expect("seed");
        dir
    }

    fn best(&self) -> PathBuf {
        self.0.join("best")
    }

    fn mute(&self) -> PathBuf {
        self.0.join("mute")
    }

    fn read(&self) -> Option<String> {
        fs::read_to_string(self.mute()).ok()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Resource, Default)]
struct Seen(Vec<(u64, u64)>);

#[derive(Resource, Default)]
struct Heard(Vec<Cue>);

fn see(sim: Res<SimRes>, mut seen: ResMut<Seen>) {
    seen.0.push((sim.sim.view().tick.0, sim.sim.state_hash()));
}

fn hear(mut cues: MessageReader<CueMessage>, mut heard: ResMut<Heard>) {
    heard.0.extend(cues.read().map(|c| c.0));
}

struct Game {
    app: App,
    w: Entity,
}

fn scenario() -> Scenario {
    parse_pack(PACK).expect("pack").scenarios[0].clone()
}

fn step() -> Duration {
    Duration::from_nanos(33_333_333)
}

fn game_at(boot: Boot, best: PathBuf, mute: PathBuf) -> Game {
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
    app.insert_resource(BestStore::at(best));
    app.insert_resource(MuteStore::at(mute));
    app.insert_resource(boot);
    app.init_resource::<Seen>().init_resource::<Heard>();
    app.add_plugins((
        InputPlugin,
        SimPlugin,
        BusPlugin,
        ClockPlugin,
        RenderPlugin,
        PackPlugin,
        BestPlugin,
        GamePlugin,
        SoundPlugin,
    ));
    app.add_systems(
        FixedUpdate,
        see.after(step_sim)
            .in_set(Ticks)
            .run_if(resource_exists::<SimRes>),
    )
    .add_systems(PostUpdate, hear);
    let mut g = Game { app, w };
    frame(&mut g, Duration::ZERO, vec![]);
    g
}

fn game(boot: Boot, dir: &Dir) -> Game {
    game_at(boot, dir.best(), dir.mute())
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

fn sim_frame(g: &mut Game, d: Duration, events: Vec<SimEvent>) -> Vec<Cue> {
    let before = heard(g).len();
    for e in events {
        g.app.world_mut().write_message(SimEventMessage(e));
    }
    frame(g, d, vec![]);
    heard(g)[before..].to_vec()
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

fn mute_lines(g: &Game) -> Vec<String> {
    lines(g)
        .into_iter()
        .filter(|l| l.starts_with("salmong: game mute="))
        .collect()
}

fn log(g: &Game) -> Vec<(Tick, Command)> {
    g.app.world().resource::<CommandLog>().0.clone()
}

fn muted(g: &Game) -> bool {
    g.app.world().resource::<Muted>().0
}

fn heard(g: &Game) -> Vec<Cue> {
    g.app.world().resource::<Heard>().0.clone()
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

fn left(w: Entity, state: ButtonState) -> WindowEvent {
    WindowEvent::MouseButtonInput(MouseButtonInput {
        button: MouseButton::Left,
        state,
        window: w,
    })
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

fn lift(w: Entity, code: KeyCode) -> WindowEvent {
    key_ev(w, code, ButtonState::Released, false)
}

fn blur(w: Entity) -> WindowEvent {
    WindowEvent::WindowFocused(WindowFocused {
        window: w,
        focused: false,
    })
}

fn press_m(g: &mut Game) {
    let w = g.w;
    frame(g, step() / 2, vec![key(w, KeyCode::KeyM)]);
}

fn start_scripted(g: &mut Game) {
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
    frame(g, step() / 2, vec![key(w, KeyCode::Space)]);
}

fn run_to_over(g: &mut Game, rate: &[Duration], mute_at: &[u64]) {
    let w = g.w;
    let mut next = 0;
    let mut i = 0;
    while screen(g) != Screen::GameOver {
        let mut events = vec![];
        if next < mute_at.len() && tick_of(g) >= mute_at[next] {
            events.push(key(w, KeyCode::KeyM));
            next += 1;
        }
        frame(g, rate[i % rate.len()], events);
        i += 1;
        assert!(i < 40_000, "no game over, tick {}", tick_of(g));
    }
    assert_eq!(next, mute_at.len());
}

fn last_line(g: &Game) -> String {
    let line = lines(g).pop().expect("a line");
    match line.find(" best=") {
        Some(i) => line[..i].to_string(),
        None => line,
    }
}

fn boarded() -> SimEvent {
    SimEvent::Boarded {
        bus: BusId(0),
        passenger: PassengerId(0),
        station: StationId(0),
    }
}

fn delivered() -> SimEvent {
    SimEvent::Delivered {
        bus: BusId(0),
        passenger: PassengerId(0),
        station: StationId(1),
    }
}

#[test]
fn cue_hand_table() {
    let station = StationId(2);
    let (bus, passenger) = (BusId(1), PassengerId(7));
    let table = [
        (SimEvent::CommandRejected, Some(Cue::Reject)),
        (SimEvent::StationSpawned { station }, Some(Cue::Station)),
        (SimEvent::OverflowStarted { station }, Some(Cue::Overflow)),
        (SimEvent::GameOver, Some(Cue::Over)),
        (
            SimEvent::Spawned {
                passenger,
                station,
                want: Shape::Star,
            },
            None,
        ),
        (
            SimEvent::Boarded {
                bus,
                passenger,
                station,
            },
            Some(Cue::Board),
        ),
        (
            SimEvent::Alighted {
                bus,
                passenger,
                station,
            },
            None,
        ),
        (
            SimEvent::Delivered {
                bus,
                passenger,
                station,
            },
            Some(Cue::Deliver),
        ),
    ];
    for (ev, want) in table {
        assert_eq!(cue(&ev), want, "{ev:?}");
    }
    for (c, _) in SOUNDS {
        assert!(volume(c) > 0.0 && volume(c) <= 1.0, "{c:?}");
    }
    assert_eq!(mute_line(true), "salmong: game mute=1");
    assert_eq!(mute_line(false), "salmong: game mute=0");
}

#[test]
fn scripted_game_with_sound_keeps_every_hash() {
    let sc = scenario();
    let want = hash_sequence(
        sc.clone(),
        u64::from(sc.seed),
        Balance::mvp(),
        &body(),
        OVER_TICK,
    );
    assert_eq!(want.len() as u64, OVER_TICK + 1);
    assert_eq!(want[OVER_TICK as usize], OVER_HASH);
    let rates: [(Vec<Duration>, bool); 2] = [
        (vec![step() / 2], false),
        (
            vec![
                step() * 2 / 5,
                step() * 13 / 10,
                step() / 20,
                step() * 26 / 10,
                step() * 9 / 10,
            ],
            true,
        ),
    ];
    for (rate, fast) in rates {
        for boot_muted in [false, true] {
            let dir = if boot_muted {
                Dir::muted("1")
            } else {
                Dir::new()
            };
            let mut g = game(Boot::Play, &dir);
            let w = g.w;
            assert_eq!(muted(&g), boot_muted);
            assert_eq!(hash_of(&g), want[0]);
            start_scripted(&mut g);
            if fast {
                frame(&mut g, step() / 2, vec![key(w, KeyCode::Digit2)]);
            }
            run_to_over(&mut g, &rate, &MUTE_TICKS);
            frames(&mut g, 20, rate[0]);
            let what = format!("{rate:?} {boot_muted}");
            let mut seen = g.app.world().resource::<Seen>().0.clone();
            for (tick, hash) in &seen {
                assert_eq!(*hash, want[*tick as usize], "tick {tick} {what}");
            }
            seen.dedup_by_key(|s| s.0);
            let ticks: Vec<u64> = seen.iter().map(|s| s.0).collect();
            assert_eq!(ticks, (1..=OVER_TICK).collect::<Vec<u64>>(), "{what}");
            assert_eq!(hash_of(&g), OVER_HASH, "{what}");
            assert_eq!(
                last_line(&g),
                over_prefix(OVER_TICK, OVER_SCORE, OVER_HASH),
                "{what}"
            );
            assert_eq!(log(&g), body(), "{what}");
            let toggles = [!boot_muted, boot_muted, !boot_muted].map(mute_line);
            assert_eq!(mute_lines(&g), toggles, "{what}");
            assert_eq!(muted(&g), !boot_muted, "{what}");
        }
    }
}

#[test]
fn cues_match_events() {
    let count = |heard: &[Cue], c: Cue| heard.iter().filter(|h| **h == c).count();

    let dir = Dir::new();
    let mut g = game(Boot::Play, &dir);
    start_scripted(&mut g);
    run_to_over(&mut g, &[step() / 2], &[]);
    frames(&mut g, 200, step() / 2);
    let all = heard(&g);
    assert!(count(&all, Cue::Board) >= 1, "{all:?}");
    assert!(count(&all, Cue::Deliver) >= 1, "{all:?}");
    assert!(count(&all, Cue::Overflow) >= 1, "{all:?}");
    assert_eq!(count(&all, Cue::Over), 1);
    assert_eq!(all.last(), Some(&Cue::Over));
    assert_eq!(count(&all, Cue::Deliver) as u64, OVER_SCORE);

    let dir = Dir::new();
    let mut g = game(Boot::Replay, &dir);
    run_to_over(&mut g, &[step() / 2], &[]);
    frames(&mut g, 200, step() / 2);
    assert_eq!(tick_of(&g), OVER_TICK);
    assert_eq!(heard(&g), vec![]);

    let dir = Dir::muted("1");
    let mut g = game(Boot::Play, &dir);
    start_scripted(&mut g);
    run_to_over(&mut g, &[step() / 2], &[]);
    frames(&mut g, 200, step() / 2);
    assert_eq!(tick_of(&g), OVER_TICK);
    assert_eq!(heard(&g), vec![]);

    let dir = Dir::new();
    let mut g = game(Boot::Play, &dir);
    start_scripted(&mut g);
    run_to_over(&mut g, &[step() / 2], &[1]);
    frames(&mut g, 200, step() / 2);
    assert_eq!(heard(&g), vec![]);
}

#[test]
fn cues_are_throttled() {
    let wait = gap(Cue::Board);
    assert_eq!(wait, Duration::from_millis(90));
    assert_eq!(gap(Cue::Deliver), wait);
    for c in [Cue::Station, Cue::Overflow, Cue::Reject, Cue::Over] {
        assert_eq!(gap(c), Duration::ZERO, "{c:?}");
    }
    let ms = Duration::from_millis;
    let dir = Dir::new();
    let mut g = game(Boot::Play, &dir);
    assert_eq!(screen(&g), Screen::Menu);
    assert_eq!(
        sim_frame(&mut g, ms(10), vec![boarded(), boarded()]),
        vec![Cue::Board]
    );
    assert_eq!(sim_frame(&mut g, ms(40), vec![boarded()]), vec![]);
    assert_eq!(
        sim_frame(&mut g, ms(49), vec![boarded(), delivered(), delivered()]),
        vec![Cue::Deliver]
    );
    assert_eq!(sim_frame(&mut g, ms(1), vec![boarded()]), vec![Cue::Board]);
    assert_eq!(sim_frame(&mut g, ms(89), vec![boarded()]), vec![]);
    assert_eq!(sim_frame(&mut g, ms(200), vec![]), vec![]);
    assert_eq!(sim_frame(&mut g, ms(1), vec![boarded()]), vec![Cue::Board]);
    assert_eq!(
        sim_frame(
            &mut g,
            ms(1),
            vec![SimEvent::CommandRejected, SimEvent::CommandRejected]
        ),
        vec![Cue::Reject]
    );
    assert_eq!(
        sim_frame(&mut g, ms(1), vec![SimEvent::CommandRejected]),
        vec![Cue::Reject]
    );
    press_m(&mut g);
    assert_eq!(
        sim_frame(&mut g, ms(500), vec![boarded(), SimEvent::GameOver]),
        vec![]
    );
    press_m(&mut g);
    assert_eq!(sim_frame(&mut g, ms(500), vec![]), vec![]);
    assert_eq!(
        sim_frame(&mut g, ms(1), vec![SimEvent::GameOver, boarded()]),
        vec![Cue::Over, Cue::Board]
    );
}

#[test]
fn mute_key_table() {
    let w = Entity::PLACEHOLDER;
    let p = ButtonState::Pressed;
    let table = [
        (KeyCode::KeyM, p, false, false, true),
        (KeyCode::KeyM, p, true, false, false),
        (KeyCode::KeyM, ButtonState::Released, false, false, false),
        (KeyCode::KeyM, p, false, true, false),
        (KeyCode::Space, p, false, false, false),
        (KeyCode::Digit1, p, false, false, false),
        (KeyCode::Digit2, p, false, false, false),
        (KeyCode::Escape, p, false, false, false),
        (KeyCode::Tab, p, false, false, false),
        (KeyCode::KeyN, p, false, false, false),
    ];
    for (code, state, repeat, held, want) in table {
        assert_eq!(
            mute_key(&key_ev(w, code, state, repeat), held),
            want,
            "{code:?} {state:?} {repeat} {held}"
        );
    }
    assert!(!mute_key(&moved(w, (1.0, 1.0)), false));

    let dir = Dir::new();
    let mut g = game(Boot::Play, &dir);
    let w = g.w;
    let d = step() / 2;
    for held in [
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
    ] {
        frame(&mut g, d, vec![key(w, held)]);
        frame(&mut g, d, vec![key(w, KeyCode::KeyM)]);
        assert!(!muted(&g), "{held:?}");
        frame(&mut g, d, vec![key(w, held), key(w, KeyCode::KeyM)]);
        assert!(!muted(&g), "{held:?}");
        frame(&mut g, d, vec![lift(w, held), key(w, KeyCode::KeyM)]);
        assert!(muted(&g), "{held:?}");
        press_m(&mut g);
        assert!(!muted(&g), "{held:?}");
    }
    frame(&mut g, d, vec![key(w, KeyCode::ShiftLeft)]);
    press_m(&mut g);
    assert!(muted(&g));
    frame(&mut g, d, vec![lift(w, KeyCode::ShiftLeft)]);
    press_m(&mut g);
    assert!(!muted(&g));
    frame(&mut g, d, vec![key(w, KeyCode::SuperLeft), blur(w)]);
    press_m(&mut g);
    assert!(muted(&g));
    for code in [
        KeyCode::Space,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Escape,
        KeyCode::Tab,
        KeyCode::KeyN,
    ] {
        frame(&mut g, d, vec![key(w, code)]);
        assert!(muted(&g), "{code:?}");
    }
    frame(
        &mut g,
        d,
        vec![
            key_ev(w, KeyCode::KeyM, ButtonState::Pressed, true),
            lift(w, KeyCode::KeyM),
        ],
    );
    assert!(muted(&g));
    frame(
        &mut g,
        d,
        vec![key(w, KeyCode::KeyM), key(w, KeyCode::KeyM)],
    );
    assert!(muted(&g));
    assert_eq!(mute_lines(&g).len(), 17);
}

#[test]
fn mute_persists() {
    let dir = Dir::new();
    let mut g = game(Boot::Play, &dir);
    assert_eq!(screen(&g), Screen::Menu);
    assert!(!muted(&g));
    assert_eq!(dir.read(), None);
    press_m(&mut g);
    assert!(muted(&g));
    assert_eq!(dir.read().as_deref(), Some("1"));
    assert_eq!(mute_lines(&g), ["salmong: game mute=1"]);
    assert!(hud_of(&g).muted);

    let mut g = game(Boot::Play, &dir);
    assert!(muted(&g));
    assert!(hud_of(&g).muted);
    assert_eq!(mute_lines(&g), Vec::<String>::new());
    press_m(&mut g);
    assert!(!muted(&g));
    assert!(!hud_of(&g).muted);
    assert_eq!(dir.read(), None);
    assert!(!dir.mute().exists());
    assert_eq!(mute_lines(&g), ["salmong: game mute=0"]);

    let g = game(Boot::Play, &dir);
    assert!(!muted(&g));

    for text in ["2", "", "1\n", "11", "0", "true"] {
        let dir = Dir::muted(text);
        let g = game(Boot::Play, &dir);
        assert!(!muted(&g), "{text:?}");
        assert!(!hud_of(&g).muted, "{text:?}");
        assert_eq!(dir.read().as_deref(), Some(text));
    }

    let dir = Dir::new();
    let mut g = game(Boot::Replay, &dir);
    assert_eq!(screen(&g), Screen::Playing);
    press_m(&mut g);
    frames(&mut g, 3, step() / 2);
    assert!(!muted(&g));
    assert_eq!(dir.read(), None);
    assert_eq!(mute_lines(&g), Vec::<String>::new());

    let dir = Dir::muted("1");
    let mut g = game(Boot::Replay, &dir);
    assert!(muted(&g));
    press_m(&mut g);
    assert!(muted(&g));
    assert_eq!(dir.read().as_deref(), Some("1"));

    let dir = Dir::new();
    let mut g = game(Boot::Off, &dir);
    assert_eq!(screen(&g), Screen::Loading);
    press_m(&mut g);
    assert!(!muted(&g));
    assert_eq!(dir.read(), None);

    let dir = Dir::new();
    fs::write(dir.best(), "salmong.best.1 1 5").expect("seed");
    let mut g = game_at(Boot::Play, dir.best(), dir.best().join("mute"));
    press_m(&mut g);
    assert!(muted(&g));
    assert!(hud_of(&g).muted);
    assert_eq!(mute_lines(&g), ["salmong: game mute=1"]);
    assert_eq!(
        fs::read_to_string(dir.best()).ok().as_deref(),
        Some("salmong.best.1 1 5")
    );
    let g = game_at(Boot::Play, dir.best(), dir.best().join("mute"));
    assert!(!muted(&g));
}

const SIGN: [u8; 3] = [0x14, 0x21, 0x3D];
const SLASH: [u8; 3] = [0xD0, 0x00, 0x00];

fn colour_at(g: &Geometry, grid: (f32, f32)) -> Option<[u8; 3]> {
    hit(g, Vec2::new(grid.0, -grid.1)).map(|c| {
        let a = Srgba::from(LinearRgba::new(c[0], c[1], c[2], c[3])).to_u8_array();
        [a[0], a[1], a[2]]
    })
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

#[test]
fn mute_sign_hand_values() {
    let grid = Vec2::new(64.0, 40.0);
    let sign = hud_geometry(&HudDraw {
        grid,
        muted: true,
        ..default()
    });
    assert_eq!(sign.positions.len(), 12);
    assert_eq!(sign.indices.len(), 18);
    for at in [(62.5, 2.0), (62.3, 2.2), (62.3, 1.8), (62.7, 1.8)] {
        assert_eq!(colour_at(&sign, at), Some(SIGN), "box {at:?}");
    }
    for at in [(63.1, 2.3), (63.2, 2.4), (63.25, 2.45), (62.9, 2.2)] {
        assert_eq!(colour_at(&sign, at), Some(SIGN), "cone {at:?}");
    }
    for at in [(62.9, 2.0), (62.2, 2.6125), (63.6, 1.3875), (62.9, 2.1)] {
        assert_eq!(colour_at(&sign, at), Some(SLASH), "slash {at:?}");
    }
    for at in [
        (62.5, 1.6),
        (62.2, 2.0),
        (63.4, 2.0),
        (62.6, 1.3),
        (63.25, 2.6),
        (63.8, 1.2),
        (62.0, 2.8),
        (61.0, 2.0),
    ] {
        assert_eq!(colour_at(&sign, at), None, "clear {at:?}");
    }
    let both = hud_geometry(&HudDraw {
        grid,
        paused: true,
        fast: true,
        muted: true,
        over: None,
    });
    assert_eq!(colour_at(&both, (61.5, 2.0)), Some(SIGN));
    assert_eq!(colour_at(&both, (61.9, 2.0)), None);
    assert_eq!(colour_at(&both, (62.5, 2.0)), Some(SIGN));
    let off = hud_geometry(&HudDraw { grid, ..default() });
    assert!(off.indices.is_empty());
    for at in [(62.5, 2.0), (63.1, 2.3), (62.9, 2.0)] {
        assert_eq!(colour_at(&off, at), None, "off {at:?}");
    }

    let dir = Dir::new();
    let mut g = game(Boot::Play, &dir);
    assert!(hud_geometry(&hud_of(&g)).indices.is_empty());
    assert_eq!(
        layer_vertices(&mut g.app, Layer::Buses).1,
        Visibility::Hidden
    );
    press_m(&mut g);
    frames(&mut g, 2, step() / 2);
    assert_eq!(
        hud_of(&g),
        HudDraw {
            grid,
            muted: true,
            ..default()
        }
    );
    let app = &mut g.app;
    assert_eq!(
        app.world_mut().query::<&Mesh2d>().iter(app.world()).count(),
        4
    );
    assert_eq!(app.world().resource::<Assets<ShapeMaterial>>().len(), 1);
    assert!(
        bus_geometry(app.world().resource::<BusDraw>())
            .positions
            .is_empty()
    );
    let (got, vis) = layer_vertices(app, Layer::Buses);
    assert_eq!(got, sign.positions);
    assert_eq!(vis, Visibility::Inherited);
    press_m(&mut g);
    frames(&mut g, 2, step() / 2);
    assert_eq!(
        layer_vertices(&mut g.app, Layer::Buses).1,
        Visibility::Hidden
    );
    assert_eq!(
        g.app
            .world_mut()
            .query::<&Mesh2d>()
            .iter(g.app.world())
            .count(),
        4
    );
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

#[test]
fn wav_files_are_canonical() {
    let mut total = 0;
    for (c, samples) in SOUNDS {
        let b = wav(c);
        total += b.len();
        assert_eq!(b.len(), 44 + 2 * samples, "{c:?}");
        assert_eq!(&b[0..4], b"RIFF", "{c:?}");
        assert_eq!(u32_at(b, 4) as usize, b.len() - 8, "{c:?}");
        assert_eq!(&b[8..12], b"WAVE", "{c:?}");
        assert_eq!(&b[12..16], b"fmt ", "{c:?}");
        assert_eq!(u32_at(b, 16), 16, "{c:?}");
        assert_eq!(u16_at(b, 20), 1, "{c:?}");
        assert_eq!(u16_at(b, 22), 1, "{c:?}");
        assert_eq!(u32_at(b, 24), 22_050, "{c:?}");
        assert_eq!(u32_at(b, 28), 44_100, "{c:?}");
        assert_eq!(u16_at(b, 32), 2, "{c:?}");
        assert_eq!(u16_at(b, 34), 16, "{c:?}");
        assert_eq!(&b[36..40], b"data", "{c:?}");
        assert_eq!(u32_at(b, 40) as usize, 2 * samples, "{c:?}");
        let pcm: Vec<i32> = b[44..]
            .chunks(2)
            .map(|s| i32::from(i16::from_le_bytes([s[0], s[1]])))
            .collect();
        assert_eq!(pcm.len(), samples, "{c:?}");
        let peak = pcm.iter().map(|v| v.abs()).max().expect("samples");
        assert!(peak <= 16_384, "{c:?} peak {peak}");
        assert!(peak >= 8_192, "{c:?} peak {peak}");
        for v in pcm[..16].iter().chain(&pcm[samples - 16..]) {
            assert!(v.abs() <= 512, "{c:?} edge {v}");
        }
    }
    assert_eq!(total, 80_524);
    assert!(total <= 100_000);
    let files: Vec<&[u8]> = SOUNDS.iter().map(|(c, _)| wav(*c)).collect();
    for (i, a) in files.iter().enumerate() {
        for b in &files[i + 1..] {
            assert_ne!(a, b);
        }
    }
}
