use bevy::prelude::*;
use salmong_core::balance::Balance;
use salmong_core::sim::{Sim, SimView};
use salmong_game::bus::{BusTrack, bus_pos};
use salmong_game::demo::{CHECK_EVERY, END_TICK, body, done_line};
use salmong_game::render::{
    BUS_COLOUR, BUS_HALF, BusDraw, Geometry, PASSENGER_COLOUR, RouteDraw, STATION_HALF,
    bus_geometry, buses, passenger_geometry, route_colour, route_geometry, routes, scene,
    station_colour, station_geometry, to_world,
};
use salmong_pack::parse_pack;
use std::process::ExitCode;

const PACK: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");
const CLEAR: [u8; 3] = [250, 128, 114];

fn rgb(c: Srgba) -> [u8; 3] {
    let a = c.to_u8_array();
    [a[0], a[1], a[2]]
}

fn inside(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let d = |p: Vec2, q: Vec2, r: Vec2| (p - r).perp_dot(q - r);
    let (d1, d2, d3) = (d(p, a, b), d(p, b, c), d(p, c, a));
    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(neg && pos)
}

fn top(layers: &[(usize, &Geometry)], world: Vec2) -> Option<(usize, [u8; 3])> {
    let mut hit = None;
    for &(layer, g) in layers {
        for t in g.indices.chunks(3) {
            let v = |i: u32| {
                let p = g.positions[i as usize];
                Vec2::new(p[0], p[1])
            };
            if inside(world, v(t[0]), v(t[1]), v(t[2])) {
                let c = g.colors[t[0] as usize];
                let s = Srgba::from(LinearRgba::new(c[0], c[1], c[2], c[3]));
                hit = Some((layer, rgb(s)));
            }
        }
    }
    hit
}

fn bus_positions(
    view: &SimView,
    sim_scenario: &salmong_core::scenario::Scenario,
) -> Vec<(salmong_core::ids::BusId, Vec2)> {
    let travel = Balance::mvp().travel_ticks;
    view.buses
        .iter()
        .map(|b| (b.id, bus_pos(b, &sim_scenario.stations, travel)))
        .collect()
}

fn main() -> ExitCode {
    let scenario = parse_pack(PACK).expect("pack").scenarios[0].clone();
    let mut sim = Sim::new(scenario.clone(), u64::from(scenario.seed), Balance::mvp());
    let body = body();
    let mut events = Vec::new();
    let mut track = BusTrack::new(Balance::mvp().travel_ticks);
    let mut bi = 0;
    for t in 0..END_TICK {
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        sim.step(&mut events);
        let view = sim.view();
        track.push(view.tick, bus_positions(&view, &scenario));
        let tick = view.tick.0;
        if tick % CHECK_EVERY == 0 {
            println!("salmong: replay tick={tick} hash={:016x}", sim.state_hash());
        }
    }
    println!("{}", done_line(END_TICK));
    println!("grid {} {}", scenario.w, scenario.h);
    let view = sim.view();
    let sc = scene(&view, &scenario.stations);
    let rd = RouteDraw {
        rev: Some(view.table_rev),
        routes: routes(&sim.snapshot(), &scenario.stations),
    };
    let bd = BusDraw {
        buses: buses(&track.at(0.0), &sc.riders),
    };
    let (gr, gs, gp, gb) = (
        route_geometry(&rd),
        station_geometry(&sc),
        passenger_geometry(&sc),
        bus_geometry(&bd),
    );
    let layers = [(0, &gr), (1, &gs), (2, &gp), (3, &gb)];
    let mut candidates: Vec<(&str, Vec2, usize, [u8; 3])> = Vec::new();
    for s in &sc.stations {
        candidates.push((
            "station",
            s.pos + Vec2::new(0.0, -0.55 * STATION_HALF / 0.8),
            1,
            rgb(station_colour(s.overflow)),
        ));
    }
    for r in &rd.routes {
        for pair in r.points.windows(2) {
            candidates.push((
                "route",
                (pair[0] + pair[1]) / 2.0,
                0,
                rgb(route_colour(r.id)),
            ));
        }
    }
    for b in &bd.buses {
        candidates.push((
            "bus",
            b.pos + Vec2::new(0.0, BUS_HALF.y * 0.6),
            3,
            rgb(BUS_COLOUR),
        ));
    }
    for p in &sc.passengers {
        candidates.push(("passenger", p.pos, 2, rgb(PASSENGER_COLOUR)));
    }
    for (x, y) in [(2.0, 2.0), (62.0, 38.0), (32.0, 2.0), (2.0, 38.0)] {
        candidates.push(("clear", Vec2::new(x, y), usize::MAX, CLEAR));
    }
    let mut seen = std::collections::BTreeSet::new();
    for (kind, grid, layer, colour) in candidates {
        let hit = top(&layers, to_world(grid));
        let ok = match hit {
            None => layer == usize::MAX,
            Some((l, c)) => l == layer && c == colour,
        };
        if ok {
            seen.insert(kind);
            println!(
                "sample {kind} {} {} {} {} {}",
                grid.x, grid.y, colour[0], colour[1], colour[2]
            );
        }
    }
    let all = ["station", "route", "bus", "passenger", "clear"];
    if all.iter().all(|k| seen.contains(k)) {
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "missing sample kinds: {:?}",
            all.iter()
                .filter(|k| !seen.contains(*k))
                .collect::<Vec<_>>()
        );
        ExitCode::FAILURE
    }
}
