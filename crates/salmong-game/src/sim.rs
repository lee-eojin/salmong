use crate::input::{PendingCommands, SimRes, apply_commands};
use bevy::prelude::*;
use salmong_core::ids::Tick;
use salmong_core::sim::{Command, SimEvent};

pub const TICK_HZ: f64 = 30.0;
pub const MAX_TICKS_PER_FRAME: u32 = 2;

#[derive(Resource, Default)]
pub struct SimEvents(pub Vec<SimEvent>);

#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ticks;

#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct SimEventMessage(pub SimEvent);

#[derive(Resource)]
pub struct ReplayFeed {
    body: Vec<(Tick, Command)>,
    next: usize,
}

impl ReplayFeed {
    pub fn new(body: Vec<(Tick, Command)>) -> Self {
        Self { body, next: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.body.len() - self.next
    }
}

pub struct SimPlugin;

impl Plugin for SimPlugin {
    fn build(&self, app: &mut App) {
        let fixed = Time::<Fixed>::from_hz(TICK_HZ);
        app.insert_resource(Time::<Virtual>::from_max_delta(
            fixed.timestep() * MAX_TICKS_PER_FRAME,
        ))
        .insert_resource(fixed)
        .init_resource::<SimEvents>()
        .add_message::<SimEventMessage>()
        .add_systems(
            FixedUpdate,
            (
                feed_replay
                    .run_if(resource_exists::<ReplayFeed>)
                    .before(apply_commands)
                    .in_set(Ticks),
                step_sim.after(apply_commands).in_set(Ticks),
                dispatch_events.after(step_sim).in_set(Ticks),
            )
                .run_if(resource_exists::<SimRes>),
        );
    }
}

fn feed_replay(
    sim: Res<SimRes>,
    mut feed: ResMut<ReplayFeed>,
    mut pending: ResMut<PendingCommands>,
) {
    if feed.next == feed.body.len() {
        return;
    }
    let tick = sim.sim.view().tick;
    while feed.next < feed.body.len() && feed.body[feed.next].0 == tick {
        let cmd = feed.body[feed.next].1.clone();
        pending.0.push(cmd);
        feed.next += 1;
    }
}

pub fn step_sim(mut sim: ResMut<SimRes>, mut events: ResMut<SimEvents>) {
    sim.sim.step(&mut events.0);
}

pub(crate) fn dispatch_events(
    mut events: ResMut<SimEvents>,
    mut out: MessageWriter<SimEventMessage>,
) {
    out.write_batch(events.0.drain(..).map(SimEventMessage));
}
