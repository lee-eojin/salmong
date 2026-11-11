use crate::best::{BestRes, record_best};
use crate::bus::BusTrack;
use crate::clock::{Clock, ClockCommand, Speed};
use crate::input::{CommandLog, PendingCommands, PointerRes, SimRes, read_pointer};
use crate::pack::ScenarioPack;
use crate::render::{HudDraw, OverInst, interpolate_buses};
use crate::sim::{ReplayFeed, SimEvents, Ticks, step_sim};
use crate::{demo, edits, stress};
use bevy::ecs::system::SystemParam;
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseButton, MouseButtonInput};
use bevy::prelude::*;
use bevy::window::WindowEvent;
use salmong_core::balance::Balance;
use salmong_core::ids::{StationId, Tick};
use salmong_core::scenario::Scenario;
use salmong_core::score::FinalScore;
use salmong_core::sim::{Command, Sim};
use salmong_pack::parse_pack;
use std::time::Duration;

const PACK_JSON: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");

pub const RESTART_DELAY: Duration = Duration::from_secs(1);

#[derive(States, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Screen {
    #[default]
    Loading,
    Menu,
    Playing,
    GameOver,
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boot {
    Off,
    Play,
    Replay,
}

impl Default for Boot {
    fn default() -> Self {
        pick(
            demo::requested() || edits::requested() || stress::requested(),
            requested(),
        )
    }
}

#[derive(Resource, Default)]
pub struct GameLog {
    pub lines: Vec<String>,
}

#[derive(Resource, Default)]
struct OverAt(Duration);

pub fn requested() -> bool {
    demo::requested_as("over")
}

pub fn pick(fixture: bool, over: bool) -> Boot {
    if fixture {
        Boot::Off
    } else if over {
        Boot::Replay
    } else {
        Boot::Play
    }
}

pub fn body() -> Vec<(Tick, Command)> {
    vec![(
        Tick(0),
        Command::AddRoute {
            stops: vec![StationId(0), StationId(1)],
        },
    )]
}

fn scenario_zero() -> Scenario {
    parse_pack(PACK_JSON)
        .expect("embedded pack must parse")
        .scenarios[0]
        .clone()
}

fn new_sim(scenario: Scenario) -> SimRes {
    let seed = u64::from(scenario.seed);
    SimRes {
        sim: Sim::new(scenario.clone(), seed, Balance::mvp()),
        scenario,
    }
}

pub fn expect(body: &[(Tick, Command)]) -> Vec<String> {
    let SimRes { mut sim, .. } = new_sim(scenario_zero());
    let mut out = Vec::new();
    let mut events = Vec::new();
    let mut bi = 0;
    loop {
        if let Some(run) = sim.final_score() {
            out.push(over_prefix(run.ticks, run.score, sim.state_hash()));
            return out;
        }
        let t = sim.view().tick.0;
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        events.clear();
        sim.step(&mut events);
        let v = sim.view();
        if v.tick.0.is_multiple_of(demo::CHECK_EVERY) {
            out.push(tick_line(v.tick.0, v.score, sim.state_hash()));
        }
    }
}

pub fn ready_line(id: u32, seed: u64, boot: Boot) -> String {
    let mode = match boot {
        Boot::Off => "off",
        Boot::Play => "play",
        Boot::Replay => "replay",
    };
    format!("salmong: game ready scenario={id} seed={seed} mode={mode}")
}

pub fn start_line() -> String {
    "salmong: game start".to_string()
}

pub fn tick_line(tick: u64, score: u64, hash: u64) -> String {
    format!("salmong: game tick={tick} score={score} hash={hash:016x}")
}

pub fn clock_line(clock: &Clock, tick: u64) -> String {
    let speed = match clock.speed() {
        Speed::X1 => 1,
        Speed::X2 => 2,
    };
    format!(
        "salmong: game clock paused={} speed={speed} tick={tick}",
        u8::from(clock.paused())
    )
}

pub fn over_prefix(tick: u64, score: u64, hash: u64) -> String {
    format!("salmong: game over tick={tick} score={score} hash={hash:016x}")
}

pub fn best_suffix(best: Option<FinalScore>) -> String {
    match best {
        Some(b) => format!(" best={},{}", b.score, b.ticks),
        None => " best=none".to_string(),
    }
}

pub fn key_command(ev: &WindowEvent, paused: bool) -> Option<ClockCommand> {
    let WindowEvent::KeyboardInput(KeyboardInput {
        key_code,
        state: ButtonState::Pressed,
        repeat: false,
        ..
    }) = ev
    else {
        return None;
    };
    match key_code {
        KeyCode::Space if paused => Some(ClockCommand::Resume),
        KeyCode::Space => Some(ClockCommand::Pause),
        KeyCode::Digit1 => Some(ClockCommand::SetSpeed(Speed::X1)),
        KeyCode::Digit2 => Some(ClockCommand::SetSpeed(Speed::X2)),
        _ => None,
    }
}

pub fn hud(
    screen: Screen,
    clock: &Clock,
    run: Option<FinalScore>,
    best: Option<FinalScore>,
    grid: Vec2,
) -> HudDraw {
    if screen == Screen::Loading {
        return HudDraw::default();
    }
    HudDraw {
        grid,
        paused: clock.paused(),
        fast: clock.speed() == Speed::X2,
        over: match (screen, run) {
            (Screen::GameOver, Some(run)) => Some(OverInst {
                score: run.score,
                best: best.map(|b| b.score),
            }),
            _ => None,
        },
    }
}

fn emit(log: &mut GameLog, line: String) {
    info!("{line}");
    log.lines.push(line);
}

fn ready(log: &mut GameLog, sim: &SimRes, boot: Boot) {
    let line = ready_line(sim.scenario.id, u64::from(sim.scenario.seed), boot);
    emit(log, line);
}

fn boot(
    mut commands: Commands,
    boot: Res<Boot>,
    pack: Option<Res<ScenarioPack>>,
    mut next: ResMut<NextState<Screen>>,
    mut log: ResMut<GameLog>,
) {
    if *boot == Boot::Off {
        return;
    }
    let Some(pack) = pack else {
        return;
    };
    let sim = new_sim(pack.0.scenarios[0].clone());
    ready(&mut log, &sim, *boot);
    commands.insert_resource(sim);
    if *boot == Boot::Replay {
        commands.insert_resource(ReplayFeed::new(body()));
        next.set(Screen::Playing);
    } else {
        next.set(Screen::Menu);
    }
}

fn over(
    sim: Res<SimRes>,
    best: Option<Res<BestRes>>,
    real: Res<Time<Real>>,
    mut at: ResMut<OverAt>,
    mut log: ResMut<GameLog>,
) {
    at.0 = real.elapsed();
    let v = sim.sim.view();
    let mut line = over_prefix(v.tick.0, v.score, sim.sim.state_hash());
    line.push_str(&best_suffix(best.and_then(|b| b.0)));
    emit(&mut log, line);
}

fn checkpoint(sim: Res<SimRes>, mut last: Local<Option<u64>>, mut log: ResMut<GameLog>) {
    let v = sim.sim.view();
    let tick = v.tick.0;
    if !tick.is_multiple_of(demo::CHECK_EVERY) || *last == Some(tick) {
        return;
    }
    *last = Some(tick);
    emit(&mut log, tick_line(tick, v.score, sim.sim.state_hash()));
}

fn read_keys(
    mut events: MessageReader<WindowEvent>,
    clock: Res<Clock>,
    mut out: MessageWriter<ClockCommand>,
) {
    let mut paused = clock.paused();
    for ev in events.read() {
        let Some(cmd) = key_command(ev, paused) else {
            continue;
        };
        match cmd {
            ClockCommand::Pause => paused = true,
            ClockCommand::Resume => paused = false,
            ClockCommand::SetSpeed(_) => {}
        }
        out.write(cmd);
    }
}

#[derive(SystemParam)]
struct Restart<'w> {
    feed: Option<Res<'w, ReplayFeed>>,
    real: Res<'w, Time<Real>>,
    at: Res<'w, OverAt>,
    pack: Option<Res<'w, ScenarioPack>>,
    sim: Option<ResMut<'w, SimRes>>,
    pointer: ResMut<'w, PointerRes>,
    pending: ResMut<'w, PendingCommands>,
    log: ResMut<'w, CommandLog>,
    events: ResMut<'w, SimEvents>,
    track: ResMut<'w, BusTrack>,
    clock: MessageWriter<'w, ClockCommand>,
}

fn is_left_press(ev: &WindowEvent) -> bool {
    matches!(
        ev,
        WindowEvent::MouseButtonInput(MouseButtonInput {
            button: MouseButton::Left,
            state: ButtonState::Pressed,
            ..
        })
    )
}

fn press(
    mut events: MessageReader<WindowEvent>,
    screen: Res<State<Screen>>,
    mut next: ResMut<NextState<Screen>>,
    mut log: ResMut<GameLog>,
    mut restart: Restart,
) {
    let pressed = events.read().filter(|ev| is_left_press(ev)).count() > 0;
    if !pressed {
        return;
    }
    match screen.get() {
        Screen::Menu => {
            next.set(Screen::Playing);
            emit(&mut log, start_line());
        }
        Screen::GameOver
            if restart.feed.is_none()
                && restart.real.elapsed().saturating_sub(restart.at.0) >= RESTART_DELAY =>
        {
            let (Some(pack), Some(sim)) = (restart.pack.as_ref(), restart.sim.as_mut()) else {
                return;
            };
            **sim = new_sim(pack.0.scenarios[0].clone());
            *restart.pointer = PointerRes::default();
            restart.pending.0.clear();
            restart.log.0.clear();
            restart.events.0.clear();
            *restart.track = BusTrack::default();
            restart.clock.write(ClockCommand::Resume);
            restart.clock.write(ClockCommand::SetSpeed(Speed::X1));
            next.set(Screen::Menu);
            ready(&mut log, sim, Boot::Play);
        }
        _ => {}
    }
}

fn follow(sim: Res<SimRes>, mut next: ResMut<NextState<Screen>>) {
    if sim.sim.final_score().is_some() {
        next.set(Screen::GameOver);
    }
}

fn log_clock(
    clock: Res<Clock>,
    sim: Option<Res<SimRes>>,
    mut last: Local<Option<(bool, Speed)>>,
    mut log: ResMut<GameLog>,
) {
    let was = last.unwrap_or((false, Speed::X1));
    let now = (clock.paused(), clock.speed());
    *last = Some(now);
    if was == now {
        return;
    }
    let tick = sim.map_or(0, |s| s.sim.view().tick.0);
    emit(&mut log, clock_line(&clock, tick));
}

fn sync_hud(
    screen: Res<State<Screen>>,
    clock: Res<Clock>,
    sim: Option<Res<SimRes>>,
    best: Option<Res<BestRes>>,
    mut draw: ResMut<HudDraw>,
) {
    let next = match sim {
        None => HudDraw::default(),
        Some(sim) => hud(
            *screen.get(),
            &clock,
            sim.sim.final_score(),
            best.and_then(|b| b.0),
            Vec2::new(f32::from(sim.scenario.w), f32::from(sim.scenario.h)),
        ),
    };
    draw.set_if_neq(next);
}

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        let no_feed = not(resource_exists::<ReplayFeed>);
        app.init_state::<Screen>()
            .init_resource::<Boot>()
            .init_resource::<GameLog>()
            .init_resource::<OverAt>()
            .configure_sets(FixedUpdate, Ticks.run_if(not(in_state(Screen::Menu))))
            .add_systems(PostStartup, boot)
            .add_systems(OnEnter(Screen::GameOver), over)
            .add_systems(
                FixedUpdate,
                checkpoint
                    .after(step_sim)
                    .in_set(Ticks)
                    .run_if(in_state(Screen::Playing)),
            )
            .add_systems(
                Update,
                (
                    read_keys
                        .run_if(no_feed.clone())
                        .run_if(not(in_state(Screen::Loading))),
                    press.after(read_pointer),
                    follow.run_if(in_state(Screen::Playing)),
                    log_clock
                        .run_if(no_feed)
                        .run_if(not(in_state(Screen::Loading))),
                    sync_hud.after(record_best).before(interpolate_buses),
                ),
            );
    }
}
