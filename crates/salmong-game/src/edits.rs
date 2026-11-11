use crate::bus::{BusTrack, bus_pos, track_buses};
use crate::clock::ClockCommand;
use crate::demo::{self, CHECK_EVERY, Demo, END_TICK, done_line};
use crate::input::{CommandLog, PendingCommands, SimRes};
use crate::pack::ScenarioPack;
use crate::render::{BusDraw, Scene, interpolate_buses};
use crate::sim::{ReplayFeed, SimEvents, dispatch_events, step_sim};
use bevy::prelude::*;
use salmong_core::balance::Balance;
use salmong_core::ids::{BusId, RouteId, StationId, Tick};
use salmong_core::sim::{Command, Sim, SimEvent, SimView};

const PACK_JSON: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");

pub const PAUSE_TICK: u64 = 390;
pub const EDIT_FRAME: u32 = 60;
pub const HOLD_FRAMES: u32 = 240;

pub fn requested() -> bool {
    demo::requested_as("edits")
}

fn stops(s: &[u16]) -> Vec<StationId> {
    s.iter().map(|&i| StationId(i)).collect()
}

pub fn body() -> Vec<(Tick, Command)> {
    vec![
        (
            Tick(0),
            Command::AddRoute {
                stops: stops(&[1, 0, 2]),
            },
        ),
        (
            Tick(240),
            Command::EditRoute {
                route: RouteId(0),
                stops: stops(&[2, 0, 1]),
            },
        ),
        (
            Tick(PAUSE_TICK),
            Command::EditRoute {
                route: RouteId(0),
                stops: stops(&[1, 0]),
            },
        ),
        (
            Tick(732),
            Command::EditRoute {
                route: RouteId(0),
                stops: stops(&[2, 3]),
            },
        ),
        (
            Tick(737),
            Command::AddRoute {
                stops: stops(&[1, 0]),
            },
        ),
        (Tick(745), Command::RemoveRoute { route: RouteId(0) }),
    ]
}

fn paused_command() -> Command {
    body()
        .into_iter()
        .find(|(t, _)| t.0 == PAUSE_TICK)
        .map(|(_, c)| c)
        .expect("body has a paused edit")
}

fn fed_body() -> Vec<(Tick, Command)> {
    body()
        .into_iter()
        .filter(|(t, _)| t.0 != PAUSE_TICK)
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub spawned: u64,
    pub rejected: u32,
    pub step_milli: u32,
}

fn steps(track: &BusTrack) -> Vec<(BusId, f32)> {
    track
        .cur()
        .iter()
        .map(|&(id, c)| {
            let d = track
                .prev()
                .iter()
                .find(|(pid, _)| *pid == id)
                .map_or(0.0, |&(_, p)| (c - p).length());
            (id, d)
        })
        .collect()
}

fn milli(d: f32) -> u32 {
    (d * 1000.0).round() as u32
}

impl Tally {
    pub fn tick(&mut self, events: &[SimEvent], track: &BusTrack) {
        for e in events {
            match e {
                SimEvent::Spawned { .. } => self.spawned += 1,
                SimEvent::CommandRejected => self.rejected += 1,
                _ => {}
            }
        }
        for (_, d) in steps(track) {
            self.step_milli = self.step_milli.max(milli(d));
        }
    }
}

pub fn stats_prefix(view: &SimView, tally: &Tally) -> String {
    let riders: usize = view.buses.iter().map(|b| b.riders.len()).sum();
    format!(
        "salmong: replay edits tick={} riders={riders} queued={} delivered={} spawned={} rejected={} step={}",
        view.tick.0, view.queued, view.score, tally.spawned, tally.rejected, tally.step_milli
    )
}

pub fn paused_line(tick: u64, rev: u64, pending: usize, frame: u32) -> String {
    format!("salmong: replay paused tick={tick} rev={rev} pending={pending} frame={frame}")
}

pub fn bound_line(tick: u64, rev: u64, pending: usize, frame: u32) -> String {
    format!("salmong: replay bound tick={tick} rev={rev} pending={pending} frame={frame}")
}

fn strip_frame(line: String) -> String {
    match line.rfind(" frame=") {
        Some(i) => line[..i].to_string(),
        None => line,
    }
}

pub fn expect() -> Vec<String> {
    let scenario = salmong_pack::parse_pack(PACK_JSON)
        .expect("embedded pack must parse")
        .scenarios[0]
        .clone();
    let balance = Balance::mvp();
    let mut sim = Sim::new(scenario.clone(), u64::from(scenario.seed), balance);
    let mut track = BusTrack::new(balance.travel_ticks);
    let mut tally = Tally::default();
    let body = body();
    let mut out = Vec::new();
    let mut bi = 0;
    for t in 0..END_TICK {
        if t == PAUSE_TICK {
            let rev = sim.view().table_rev;
            out.push(strip_frame(paused_line(t, rev, 1, 0)));
        }
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        let mut events = Vec::new();
        sim.step(&mut events);
        let view = sim.view();
        let cur = view
            .buses
            .iter()
            .map(|b| (b.id, bus_pos(b, &scenario.stations, balance.travel_ticks)))
            .collect();
        track.push(view.tick, cur);
        tally.tick(&events, &track);
        if t == PAUSE_TICK {
            out.push(strip_frame(bound_line(t, view.table_rev, 0, 0)));
        }
        let tick = view.tick.0;
        if tick.is_multiple_of(CHECK_EVERY) {
            out.push(format!(
                "salmong: replay tick={tick} hash={:016x}",
                sim.state_hash()
            ));
            if tick == END_TICK {
                out.push(done_line(END_TICK));
            }
            out.push(stats_prefix(&view, &tally));
        }
    }
    out
}

#[derive(Resource, Default)]
pub struct Edits {
    pub tally: Tally,
    pub draw_milli: u32,
    pub draw_over: u32,
    pub lost: u32,
    pub held: u32,
    hold: Option<u32>,
    awaiting: bool,
    last: Vec<(BusId, Vec2)>,
    carry: Vec<(BusId, f32)>,
    ran: Vec<(BusId, f32)>,
    latest: Vec<(BusId, f32)>,
    ticked: bool,
}

pub fn start(mut commands: Commands, pack: Res<ScenarioPack>) {
    let scenario = pack.0.scenarios[0].clone();
    let seed = u64::from(scenario.seed);
    commands.insert_resource(SimRes {
        sim: Sim::new(scenario.clone(), seed, Balance::mvp()),
        scenario,
    });
    commands.insert_resource(ReplayFeed::new(fed_body()));
    commands.insert_resource(Demo::default());
    commands.insert_resource(Edits::default());
}

fn emit(demo: &mut Demo, line: String) {
    info!("{line}");
    demo.lines.push(line);
}

fn find(list: &[(BusId, f32)], id: BusId) -> f32 {
    list.iter().find(|(i, _)| *i == id).map_or(0.0, |&(_, d)| d)
}

#[allow(clippy::too_many_arguments)]
fn tally(
    sim: Res<SimRes>,
    events: Res<SimEvents>,
    track: Res<BusTrack>,
    log: Res<CommandLog>,
    pending: Res<PendingCommands>,
    mut edits: ResMut<Edits>,
    mut demo: ResMut<Demo>,
    mut fixed: ResMut<Time<Fixed>>,
    mut clock: MessageWriter<ClockCommand>,
) {
    let edits = &mut *edits;
    edits.tally.tick(&events.0, &track);
    let tick_steps = steps(&track);
    for &(id, d) in &tick_steps {
        match edits.ran.iter_mut().find(|(i, _)| *i == id) {
            Some((_, r)) => *r += d,
            None => edits.ran.push((id, d)),
        }
    }
    edits.latest = tick_steps;
    edits.ticked = true;
    let view = sim.sim.view();
    if edits.awaiting {
        let bound = log.0.last().map_or(0, |(t, _)| t.0);
        let line = bound_line(bound, view.table_rev, pending.0.len(), demo.frames);
        emit(&mut demo, line);
        edits.awaiting = false;
    }
    let tick = view.tick.0;
    if tick.is_multiple_of(CHECK_EVERY) && tick <= END_TICK {
        let line = format!(
            "{} draw={} draw_over={} lost={}",
            stats_prefix(&view, &edits.tally),
            edits.draw_milli,
            edits.draw_over,
            edits.lost
        );
        emit(&mut demo, line);
    }
    if tick == PAUSE_TICK && edits.hold.is_none() && !edits.awaiting {
        clock.write(ClockCommand::Pause);
        let o = fixed.overstep();
        fixed.discard_overstep(o);
        edits.hold = Some(0);
    }
}

fn hold(
    sim: Res<SimRes>,
    mut edits: ResMut<Edits>,
    mut demo: ResMut<Demo>,
    mut pending: ResMut<PendingCommands>,
    mut clock: MessageWriter<ClockCommand>,
) {
    let Some(n) = edits.hold else {
        return;
    };
    let n = n + 1;
    edits.held += 1;
    edits.hold = Some(n);
    if n == EDIT_FRAME {
        pending.0.push(paused_command());
        let view = sim.sim.view();
        let line = paused_line(view.tick.0, view.table_rev, pending.0.len(), demo.frames);
        emit(&mut demo, line);
    }
    if n == HOLD_FRAMES {
        clock.write(ClockCommand::Resume);
        edits.hold = None;
        edits.awaiting = true;
    }
}

fn measure(sim: Res<SimRes>, draw: Res<BusDraw>, scene: Res<Scene>, mut edits: ResMut<Edits>) {
    let edits = &mut *edits;
    for b in &draw.buses {
        let Some(&(_, last)) = edits.last.iter().find(|(id, _)| *id == b.id) else {
            continue;
        };
        let d = (b.pos - last).length();
        edits.draw_milli = edits.draw_milli.max(milli(d));
        let allowed = find(&edits.carry, b.id) + find(&edits.ran, b.id) + 1e-3;
        if d > allowed {
            edits.draw_over += 1;
        }
    }
    let riders: u64 = draw.buses.iter().map(|b| u64::from(b.riders)).sum();
    let badges: u64 = scene.badges.iter().map(|b| u64::from(b.extra)).sum();
    let shown = riders + scene.passengers.len() as u64 + badges + sim.sim.view().score;
    if shown != edits.tally.spawned {
        edits.lost += 1;
    }
    if edits.ticked {
        edits.carry = std::mem::take(&mut edits.latest);
        edits.ticked = false;
    }
    edits.ran.clear();
    edits.last = draw.buses.iter().map(|b| (b.id, b.pos)).collect();
}

pub struct EditsPlugin;

impl Plugin for EditsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, start.run_if(requested))
            .add_systems(
                FixedUpdate,
                tally
                    .after(step_sim)
                    .after(track_buses)
                    .after(demo::report)
                    .before(dispatch_events)
                    .run_if(resource_exists::<Edits>)
                    .run_if(resource_exists::<SimRes>),
            )
            .add_systems(
                First,
                hold.run_if(resource_exists::<Edits>)
                    .run_if(resource_exists::<SimRes>),
            )
            .add_systems(
                Update,
                measure
                    .after(interpolate_buses)
                    .run_if(resource_exists::<Edits>)
                    .run_if(resource_exists::<SimRes>),
            );
    }
}
