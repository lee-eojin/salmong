use crate::bus::{BusTrack, track_buses};
use crate::clock::{Clock, ClockCommand, Speed};
use crate::demo;
use crate::edits;
use crate::input::SimRes;
use crate::sim::{ReplayFeed, dispatch_events, step_sim};
use bevy::prelude::*;
use salmong_core::balance::Balance;
use salmong_core::ids::{RouteId, StationId, Tick};
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{BusMode, Command, Sim, SimView};

pub const WARMUP_TICK: u64 = 900;
pub const END_TICK: u64 = 8_100;
pub const CHECK_EVERY: u64 = 300;
pub const EDIT_FIRST: u64 = 1_050;
pub const EDIT_EVERY: u64 = 300;
pub const EDITS: u64 = 24;
pub const ROUTES: usize = 8;
pub const HUB: StationId = StationId(63);
const STRIDES: [usize; ROUTES] = [1, 5, 7, 11, 13, 17, 19, 23];

pub fn selected(query: &str) -> Option<u64> {
    let pairs: Vec<&str> = query
        .strip_prefix('?')
        .unwrap_or(query)
        .split('&')
        .collect();
    if !pairs.contains(&"bench=stress") {
        return None;
    }
    pairs
        .iter()
        .find_map(|p| p.strip_prefix("seed="))
        .and_then(|s| s.parse().ok())
}

#[cfg(target_arch = "wasm32")]
fn query() -> String {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default()
}

#[cfg(not(target_arch = "wasm32"))]
fn query() -> String {
    std::env::var("SALMONG_BENCH").unwrap_or_default()
}

fn requested_seed() -> Option<u64> {
    if demo::requested() || edits::requested() {
        return None;
    }
    selected(&query())
}

pub fn requested() -> bool {
    requested_seed().is_some()
}

pub fn scenario() -> Scenario {
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate: 100 }],
        stations: (0..64u16)
            .map(|i| StationSpec {
                x: 4 + 7 * (i % 8),
                y: 2 + 5 * (i / 8),
                shape: if StationId(i) == HUB {
                    Shape::Star
                } else {
                    Shape::ALL[usize::from(i % 3)]
                },
                at_tick: 0,
                weight: 5,
            })
            .collect(),
    }
}

pub fn stops(route: usize, hub: bool) -> Vec<StationId> {
    (0..64)
        .map(|i| StationId(((i * STRIDES[route] + route) % 64) as u16))
        .filter(|&s| hub || s != HUB)
        .collect()
}

pub fn body() -> Vec<(Tick, Command)> {
    let mut out: Vec<(Tick, Command)> = (0..ROUTES)
        .map(|r| {
            (
                Tick(0),
                Command::AddRoute {
                    stops: stops(r, false),
                },
            )
        })
        .collect();
    for k in 0..EDITS {
        out.push((
            Tick(EDIT_FIRST + EDIT_EVERY * k),
            Command::EditRoute {
                route: RouteId(7),
                stops: stops(7, k.is_multiple_of(2)),
            },
        ));
    }
    out
}

pub fn alive(view: &SimView) -> usize {
    view.queued + view.buses.iter().map(|b| b.riders.len()).sum::<usize>()
}

pub fn in_service(view: &SimView) -> usize {
    view.buses
        .iter()
        .filter(|b| matches!(b.mode, BusMode::Active { .. }))
        .count()
}

pub fn check_line(
    tick: u64,
    hash: u64,
    alive: usize,
    buses: usize,
    rev: u64,
    speed: u32,
) -> String {
    format!(
        "salmong: bench tick={tick} hash={hash:016x} alive={alive} buses={buses} rev={rev} speed={speed}"
    )
}

pub fn steady_line(tick: u64) -> String {
    format!("salmong: bench steady tick={tick}")
}

pub fn done_line(tick: u64) -> String {
    format!("salmong: bench done tick={tick}")
}

pub fn header_line(seed: u64) -> String {
    format!("salmong: bench stress seed={seed} warmup={WARMUP_TICK} end={END_TICK}")
}

fn tail(tick: u64) -> Vec<String> {
    let mut out = Vec::new();
    if tick == WARMUP_TICK {
        out.push(steady_line(tick));
    }
    if tick == END_TICK {
        out.push(done_line(tick));
    }
    out
}

pub fn expect(seed: u64) -> Vec<String> {
    let mut sim = Sim::new(scenario(), seed, Balance::bench());
    let body = body();
    let mut events = Vec::new();
    let mut bi = 0;
    let mut out = Vec::new();
    for t in 0..END_TICK {
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        sim.step(&mut events);
        events.clear();
        let tick = t + 1;
        if tick.is_multiple_of(CHECK_EVERY) {
            let v = sim.view();
            let speed = if tick <= WARMUP_TICK { 1 } else { 2 };
            out.push(check_line(
                tick,
                sim.state_hash(),
                alive(&v),
                in_service(&v),
                v.table_rev,
                speed,
            ));
            out.extend(tail(tick));
        }
    }
    out
}

#[derive(Resource, Default)]
pub struct Stress {
    pub frames: u32,
    pub ticks: u64,
    pub lines: Vec<String>,
}

pub fn start(mut commands: Commands) {
    let seed = requested_seed().unwrap_or(1);
    commands.insert_resource(SimRes {
        sim: Sim::new(scenario(), seed, Balance::bench()),
        scenario: scenario(),
    });
    commands.insert_resource(ReplayFeed::new(body()));
    commands.insert_resource(BusTrack::new(Balance::bench().travel_ticks));
    commands.insert_resource(Stress::default());
    info!("{}", header_line(seed));
}

fn emit(stress: &mut Stress, line: String) {
    info!("{line}");
    stress.lines.push(line);
}

pub(crate) fn report(
    sim: Res<SimRes>,
    clock: Res<Clock>,
    mut stress: ResMut<Stress>,
    mut fixed: ResMut<Time<Fixed>>,
    mut commands: MessageWriter<ClockCommand>,
) {
    stress.ticks += 1;
    let tick = stress.ticks;
    if !tick.is_multiple_of(CHECK_EVERY) || tick > END_TICK {
        return;
    }
    let v = sim.sim.view();
    let speed = match clock.speed() {
        Speed::X1 => 1,
        Speed::X2 => 2,
    };
    let line = check_line(
        v.tick.0,
        sim.sim.state_hash(),
        alive(&v),
        in_service(&v),
        v.table_rev,
        speed,
    );
    let frame = stress.frames;
    emit(&mut stress, format!("{line} frame={frame}"));
    for l in tail(tick) {
        emit(&mut stress, l);
    }
    if tick == WARMUP_TICK {
        commands.write(ClockCommand::SetSpeed(Speed::X2));
    }
    if tick == END_TICK {
        commands.write(ClockCommand::Pause);
        let o = fixed.overstep();
        fixed.discard_overstep(o);
    }
}

fn count_frames(mut stress: ResMut<Stress>) {
    stress.frames += 1;
}

pub struct StressPlugin;

impl Plugin for StressPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, start.run_if(requested))
            .add_systems(
                FixedUpdate,
                report
                    .after(step_sim)
                    .after(track_buses)
                    .before(dispatch_events)
                    .run_if(resource_exists::<Stress>)
                    .run_if(resource_exists::<SimRes>),
            )
            .add_systems(First, count_frames.run_if(resource_exists::<Stress>));
    }
}
