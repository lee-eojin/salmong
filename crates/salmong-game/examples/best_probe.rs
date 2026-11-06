use bevy::log::LogPlugin;
use bevy::prelude::*;
use salmong_core::balance::Balance;
use salmong_core::ids::StationId;
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::score::FinalScore;
use salmong_core::sim::{Command, Sim};
use salmong_game::best::{BestPlugin, BestRes};
use salmong_game::input::SimRes;

fn scenario() -> Scenario {
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate: 5 }],
        stations: [
            (12, 7, Shape::Circle),
            (30, 22, Shape::Triangle),
            (48, 12, Shape::Square),
        ]
        .into_iter()
        .map(|(x, y, shape)| StationSpec {
            x,
            y,
            shape,
            at_tick: 0,
            weight: 3,
        })
        .collect(),
    }
}

fn balance() -> Balance {
    Balance {
        travel_ticks: 1,
        service_ticks: 1,
        station_capacity: 2,
        overflow_limit: 40,
        spawn_unit: 30,
        ..Balance::mvp()
    }
}

fn line(best: Option<FinalScore>) -> String {
    best.map_or_else(|| "none".to_string(), FinalScore::encode)
}

fn main() {
    let mut app = App::new();
    app.add_plugins((LogPlugin::default(), BestPlugin));
    app.update();
    info!(
        "best-probe loaded {}",
        line(app.world().resource::<BestRes>().0)
    );
    let mut sim = Sim::new(scenario(), 1, balance());
    sim.queue(Command::AddRoute {
        stops: vec![StationId(0), StationId(1)],
    });
    let mut events = Vec::new();
    while sim.final_score().is_none() {
        sim.step(&mut events);
    }
    info!("best-probe run {}", line(sim.final_score()));
    app.insert_resource(SimRes {
        sim,
        scenario: scenario(),
    });
    app.update();
    info!(
        "best-probe best {}",
        line(app.world().resource::<BestRes>().0)
    );
}
