use crate::input::SimRes;
use crate::sim::{Ticks, step_sim};
use bevy::prelude::*;
use salmong_core::balance::Balance;
use salmong_core::ids::{BusId, StationId, Tick};
use salmong_core::scenario::StationSpec;
use salmong_core::sim::{BusPhase, BusView};

fn centre(stations: &[StationSpec], s: StationId) -> Vec2 {
    let st = &stations[usize::from(s.0)];
    Vec2::new(f32::from(st.x), f32::from(st.y))
}

pub fn bus_pos(bus: &BusView, stations: &[StationSpec], travel_ticks: u32) -> Vec2 {
    match bus.phase {
        BusPhase::Moving { from, to, progress } => {
            let a = centre(stations, from);
            let b = centre(stations, to);
            a + (b - a) * (progress as f32 / travel_ticks as f32)
        }
        BusPhase::Alighting | BusPhase::Boarding => centre(stations, bus.at),
    }
}

pub fn blend(prev: Option<Vec2>, cur: Vec2, alpha: f32) -> Vec2 {
    match prev {
        None => cur,
        Some(p) => p + (cur - p) * alpha.clamp(0.0, 1.0),
    }
}

#[derive(Resource)]
pub struct BusTrack {
    travel_ticks: u32,
    tick: Tick,
    prev: Vec<(BusId, Vec2)>,
    cur: Vec<(BusId, Vec2)>,
}

impl Default for BusTrack {
    fn default() -> Self {
        Self::new(Balance::mvp().travel_ticks)
    }
}

impl BusTrack {
    pub fn new(travel_ticks: u32) -> Self {
        Self {
            travel_ticks,
            tick: Tick(0),
            prev: Vec::new(),
            cur: Vec::new(),
        }
    }

    pub fn push(&mut self, tick: Tick, cur: Vec<(BusId, Vec2)>) {
        if tick.0 == self.tick.0 + 1 {
            self.prev = std::mem::take(&mut self.cur);
        } else {
            self.prev = cur.clone();
        }
        self.cur = cur;
        self.tick = tick;
    }

    pub fn tick(&self) -> Tick {
        self.tick
    }

    pub fn prev(&self) -> &[(BusId, Vec2)] {
        &self.prev
    }

    pub fn cur(&self) -> &[(BusId, Vec2)] {
        &self.cur
    }

    pub fn at(&self, alpha: f32) -> Vec<(BusId, Vec2)> {
        self.cur
            .iter()
            .map(|&(id, c)| {
                let p = self
                    .prev
                    .iter()
                    .find(|(pid, _)| *pid == id)
                    .map(|&(_, p)| p);
                (id, blend(p, c, alpha))
            })
            .collect()
    }
}

pub fn track_buses(sim: Res<SimRes>, mut track: ResMut<BusTrack>) {
    let v = sim.sim.view();
    let travel = track.travel_ticks;
    let cur = v
        .buses
        .iter()
        .map(|b| (b.id, bus_pos(b, &sim.scenario.stations, travel)))
        .collect();
    track.push(v.tick, cur);
}

pub struct BusPlugin;

impl Plugin for BusPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BusTrack>().add_systems(
            FixedUpdate,
            track_buses
                .after(step_sim)
                .run_if(resource_exists::<SimRes>)
                .in_set(Ticks),
        );
    }
}
