use bevy::prelude::*;
use salmong_core::balance::Balance;
use salmong_core::scenario::Shape;
use salmong_core::sim::Sim;
use salmong_game::bus::{BusTrack, bus_pos};
use salmong_game::render::{
    BADGE_COLOUR, BUS_COLOUR, BUS_HALF, BusDraw, Geometry, PASSENGER_COLOUR, RouteDraw,
    bus_geometry, buses, hit, passenger_geometry, route_geometry, routes, scene, station_colour,
    station_geometry, to_world,
};
use salmong_game::stress::{END_TICK, body, expect, scenario};
use std::collections::BTreeSet;
use std::process::ExitCode;

const CLEAR: [u8; 3] = [250, 128, 114];
const SEED: u64 = 1;
const MARGIN: f32 = 0.06;

type Candidate = (&'static str, Option<Shape>, Vec2, usize, [u8; 3]);

fn rgb(c: Srgba) -> [u8; 3] {
    let a = c.to_u8_array();
    [a[0], a[1], a[2]]
}

fn top(layers: &[(usize, &Geometry)], world: Vec2) -> Option<(usize, [u8; 3])> {
    let mut out = None;
    for &(layer, g) in layers {
        if let Some(c) = hit(g, world) {
            let s = Srgba::from(LinearRgba::new(c[0], c[1], c[2], c[3]));
            out = Some((layer, rgb(s)));
        }
    }
    out
}

fn settled(layers: &[(usize, &Geometry)], grid: Vec2) -> Option<Option<(usize, [u8; 3])>> {
    let centre = top(layers, to_world(grid));
    let steady = (-1..=1).all(|dx| {
        (-1..=1).all(|dy| {
            let d = Vec2::new(dx as f32, dy as f32) * MARGIN;
            top(layers, to_world(grid + d)) == centre
        })
    });
    steady.then_some(centre)
}

fn shape_name(s: Shape) -> &'static str {
    match s {
        Shape::Circle => "circle",
        Shape::Triangle => "triangle",
        Shape::Square => "square",
        Shape::Star => "star",
    }
}

fn main() -> ExitCode {
    for l in expect(SEED) {
        println!("{l}");
    }
    let sc = scenario();
    let travel = Balance::bench().travel_ticks;
    let mut sim = Sim::new(sc.clone(), SEED, Balance::bench());
    let body = body();
    let mut events = Vec::new();
    let mut track = BusTrack::new(travel);
    let mut bi = 0;
    for t in 0..END_TICK {
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        sim.step(&mut events);
        events.clear();
        if t + 2 >= END_TICK {
            let view = sim.view();
            track.push(
                view.tick,
                view.buses
                    .iter()
                    .map(|b| (b.id, bus_pos(b, &sc.stations, travel)))
                    .collect(),
            );
        }
    }
    println!("grid {} {}", sc.w, sc.h);
    let view = sim.view();
    let scn = scene(&view, &sc.stations);
    let rd = RouteDraw {
        rev: Some(view.table_rev),
        routes: routes(&sim.snapshot(), &sc.stations),
    };
    let bd = BusDraw {
        buses: buses(&track.at(0.0), &scn.riders),
    };
    let (gr, gs, gp, gb) = (
        route_geometry(&rd),
        station_geometry(&scn),
        passenger_geometry(&scn),
        bus_geometry(&bd),
    );
    let layers = [(0, &gr), (1, &gs), (2, &gp), (3, &gb)];
    let mut candidates: Vec<Candidate> = Vec::new();
    for s in &scn.stations {
        let colour = rgb(station_colour(s.overflow));
        candidates.push((
            "station",
            Some(s.shape),
            s.pos + Vec2::new(0.0, -0.55),
            1,
            colour,
        ));
        let corner = s.pos + Vec2::new(0.68, -0.68);
        let expected = if s.shape == Shape::Square {
            Some((1, colour))
        } else {
            let below = [(0, &gr)];
            top(&below, to_world(corner))
        };
        match expected {
            Some((l, c)) => candidates.push(("corner", Some(s.shape), corner, l, c)),
            None => candidates.push(("corner", Some(s.shape), corner, usize::MAX, CLEAR)),
        }
    }
    for p in scn.passengers.iter().filter(|p| p.slot == 0) {
        candidates.push(("passenger", None, p.pos, 2, rgb(PASSENGER_COLOUR)));
    }
    for b in &scn.badges {
        candidates.push(("badge", None, b.pos, 2, rgb(BADGE_COLOUR)));
    }
    for b in &bd.buses {
        candidates.push((
            "bus",
            None,
            b.pos + Vec2::new(0.0, BUS_HALF.y * 0.6),
            3,
            rgb(BUS_COLOUR),
        ));
    }
    for (x, y) in [(0.5, 0.5), (63.5, 39.5), (63.5, 0.5), (0.5, 39.5)] {
        candidates.push(("clear", None, Vec2::new(x, y), usize::MAX, CLEAR));
    }
    let mut seen = BTreeSet::new();
    for (kind, shape, grid, layer, colour) in candidates {
        let ok = match settled(&layers, grid) {
            None => false,
            Some(None) => layer == usize::MAX,
            Some(Some((l, c))) => l == layer && c == colour,
        };
        if ok {
            seen.insert((kind, shape.map(shape_name)));
            println!(
                "sample {kind} {} {} {} {} {}",
                grid.x, grid.y, colour[0], colour[1], colour[2]
            );
        }
    }
    let kinds: BTreeSet<&str> = seen.iter().map(|(k, _)| *k).collect();
    let mut missing: Vec<String> = ["station", "corner", "passenger", "badge", "bus", "clear"]
        .into_iter()
        .filter(|k| !kinds.contains(k))
        .map(str::to_string)
        .collect();
    for kind in ["station", "corner"] {
        for s in Shape::ALL.into_iter().chain([Shape::Star]) {
            if !seen.contains(&(kind, Some(shape_name(s)))) {
                missing.push(format!("{kind} {}", shape_name(s)));
            }
        }
    }
    if missing.is_empty() {
        ExitCode::SUCCESS
    } else {
        eprintln!("missing samples: {missing:?}");
        ExitCode::FAILURE
    }
}
