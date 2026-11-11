use crate::bus::track_buses;
use crate::clock::ClockCommand;
use crate::input::SimRes;
use crate::pack::ScenarioPack;
use crate::sim::{ReplayFeed, step_sim};
use bevy::prelude::*;
use salmong_core::balance::Balance;
use salmong_core::ids::{RouteId, StationId, Tick};
use salmong_core::sim::{Command, Sim};

pub const CHECK_EVERY: u64 = 150;
pub const END_TICK: u64 = 900;

pub fn selects(query: &str, name: &str) -> bool {
    let want = format!("replay={name}");
    query
        .strip_prefix('?')
        .unwrap_or(query)
        .split('&')
        .any(|pair| pair == want)
}

pub fn selects_demo(query: &str) -> bool {
    selects(query, "demo")
}

#[cfg(target_arch = "wasm32")]
pub fn requested_as(name: &str) -> bool {
    web_sys::window()
        .and_then(|w| w.location().search().ok())
        .is_some_and(|q| selects(&q, name))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn requested_as(name: &str) -> bool {
    std::env::var("SALMONG_REPLAY").is_ok_and(|v| v == name)
}

pub fn requested() -> bool {
    requested_as("demo")
}

fn stops(s: &[u16]) -> Vec<StationId> {
    s.iter().map(|&i| StationId(i)).collect()
}

pub fn body() -> Vec<(Tick, Command)> {
    vec![
        (
            Tick(0),
            Command::AddRoute {
                stops: stops(&[0, 1]),
            },
        ),
        (
            Tick(300),
            Command::EditRoute {
                route: RouteId(0),
                stops: stops(&[1, 0]),
            },
        ),
        (
            Tick(620),
            Command::AddRoute {
                stops: stops(&[1, 3]),
            },
        ),
    ]
}

pub fn line(tick: u64, hash: u64, frame: u32) -> String {
    format!("salmong: replay tick={tick} hash={hash:016x} frame={frame}")
}

pub fn done_line(tick: u64) -> String {
    format!("salmong: replay done tick={tick}")
}

#[derive(Resource, Default)]
pub struct Demo {
    pub frames: u32,
    pub lines: Vec<String>,
}

pub fn start(mut commands: Commands, pack: Res<ScenarioPack>) {
    let scenario = pack.0.scenarios[0].clone();
    let seed = u64::from(scenario.seed);
    commands.insert_resource(SimRes {
        sim: Sim::new(scenario.clone(), seed, Balance::mvp()),
        scenario,
    });
    commands.insert_resource(ReplayFeed::new(body()));
    commands.insert_resource(Demo::default());
}

pub(crate) fn report(
    sim: Res<SimRes>,
    mut demo: ResMut<Demo>,
    mut fixed: ResMut<Time<Fixed>>,
    mut clock: MessageWriter<ClockCommand>,
) {
    let tick = sim.sim.view().tick.0;
    if !tick.is_multiple_of(CHECK_EVERY) || tick > END_TICK {
        return;
    }
    let l = line(tick, sim.sim.state_hash(), demo.frames);
    info!("{l}");
    demo.lines.push(l);
    if tick == END_TICK {
        let d = done_line(tick);
        info!("{d}");
        demo.lines.push(d);
        clock.write(ClockCommand::Pause);
        let o = fixed.overstep();
        fixed.discard_overstep(o);
    }
}

fn count_frames(mut demo: ResMut<Demo>) {
    demo.frames += 1;
}

pub struct DemoPlugin;

impl Plugin for DemoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, start.run_if(requested))
            .add_systems(
                FixedUpdate,
                report
                    .after(step_sim)
                    .after(track_buses)
                    .run_if(resource_exists::<Demo>)
                    .run_if(resource_exists::<SimRes>),
            )
            .add_systems(First, count_frames.run_if(resource_exists::<Demo>));
    }
}
