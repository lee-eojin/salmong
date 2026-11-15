use bevy::prelude::*;
use salmong_core::balance::Balance;
use salmong_core::ids::{BusId, Tick};
use salmong_core::scenario::{Scenario, Shape};
use salmong_core::sim::{Command, Sim, SimView};
use salmong_game::bus::{BusTrack, bus_pos};
use salmong_game::game::{body, expect};
use salmong_game::render::{
    BEST_COLOUR, BusDraw, DIGIT_COLOUR, DIGIT_STEP, DOT_COLOUR, Geometry, HudDraw, INK, OverInst,
    PANEL_COLOUR, PANEL_HALF, PASSENGER_COLOUR, PendingDraw, RING_COLOUR, RING_HALF, RouteDraw,
    best_row, bus_colour, buses, buses_layer, digit_segments, digits, hit, passenger_geometry,
    ring_fraction, route_colour, routes, routes_layer, run_row, scene, segment_centres,
    station_colour, station_geometry, to_world,
};
use salmong_pack::parse_pack;
use std::collections::BTreeSet;
use std::f32::consts::TAU;
use std::process::ExitCode;

const PACK: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");
const CLEAR: [u8; 3] = [250, 128, 114];
const BODY: Vec2 = Vec2::new(0.0, 0.15);
const RING_AT: f32 = 0.9;
const RING_MIN: f32 = 0.1;
const KINDS: [&str; 13] = [
    "station",
    "outline",
    "ring",
    "dot",
    "bus",
    "route",
    "passenger",
    "panel",
    "seg_on",
    "seg_off",
    "icon_run",
    "icon_best",
    "clear",
];

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

fn outline_rise(shape: Shape) -> f32 {
    match shape {
        Shape::Circle | Shape::Square => 0.7,
        Shape::Triangle => 0.64,
        Shape::Star => 0.65,
    }
}

fn ring_point(centre: Vec2, fraction: f32) -> Vec2 {
    let a = TAU * fraction / 2.0;
    centre + Vec2::new(a.sin(), -a.cos()) * RING_HALF * RING_AT
}

fn positions(view: &SimView, scenario: &Scenario) -> Vec<(BusId, Vec2)> {
    let travel = Balance::mvp().travel_ticks;
    view.buses
        .iter()
        .map(|b| (b.id, bus_pos(b, &scenario.stations, travel)))
        .collect()
}

fn play(body: &[(Tick, Command)], scenario: &Scenario) -> (Sim, BusTrack) {
    let mut sim = Sim::new(scenario.clone(), u64::from(scenario.seed), Balance::mvp());
    let mut track = BusTrack::new(Balance::mvp().travel_ticks);
    let mut events = Vec::new();
    let mut bi = 0;
    while sim.final_score().is_none() {
        let t = sim.view().tick.0;
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        events.clear();
        sim.step(&mut events);
        let view = sim.view();
        track.push(view.tick, positions(&view, scenario));
    }
    (sim, track)
}

fn digit_candidates(out: &mut Vec<(&'static str, Vec2, usize, [u8; 3])>, first: Vec2, value: u64) {
    for (k, d) in digits(value).into_iter().enumerate() {
        let centre = first + Vec2::new(DIGIT_STEP * k as f32, 0.0);
        for (lit, (c, _)) in digit_segments(d).into_iter().zip(segment_centres(centre)) {
            if lit {
                out.push(("seg_on", c, 3, rgb(DIGIT_COLOUR)));
            } else {
                out.push(("seg_off", c, 3, rgb(PANEL_COLOUR)));
            }
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let idle = args.iter().any(|a| a == "idle");
    let best_arg = args
        .iter()
        .position(|a| a == "--best")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let body = if idle { Vec::new() } else { body() };
    for line in expect(&body) {
        println!("{line}");
    }
    let scenario = parse_pack(PACK).expect("pack").scenarios[0].clone();
    let (sim, track) = play(&body, &scenario);
    let run = sim.final_score().expect("game over");
    let best = match best_arg.as_deref() {
        None => Some(run.score),
        Some("none") => None,
        Some(n) => Some(n.parse::<u64>().expect("--best takes a number or none")),
    };
    let grid = Vec2::new(f32::from(scenario.w), f32::from(scenario.h));
    let limit = Balance::mvp().overflow_limit;
    println!("grid {} {}", scenario.w, scenario.h);
    let view = sim.view();
    let sc = scene(&view, &scenario.stations);
    let rd = RouteDraw {
        rev: Some(view.table_rev),
        grid,
        routes: routes(&sim.snapshot(), &scenario.stations),
    };
    let bd = BusDraw {
        buses: buses(&track.at(1.0), &sc.riders),
    };
    let hud = HudDraw {
        grid,
        paused: false,
        fast: false,
        over: Some(OverInst {
            score: run.score,
            best,
        }),
    };
    let (gr, gs, gp, gb) = (
        routes_layer(&rd, &PendingDraw::default()),
        station_geometry(&sc, limit),
        passenger_geometry(&sc),
        buses_layer(&bd, &hud),
    );
    let layers = [(0, &gr), (1, &gs), (2, &gp), (3, &gb)];
    let mut candidates: Vec<(&str, Vec2, usize, [u8; 3])> = Vec::new();
    for s in &sc.stations {
        candidates.push(("station", s.pos, 1, rgb(station_colour(s.overflow))));
        let band = s.pos - Vec2::new(0.0, outline_rise(s.shape));
        candidates.push(("outline", band, 1, rgb(INK)));
        let fraction = ring_fraction(s.overflow, limit);
        if s.overflow > 0 && fraction >= RING_MIN {
            let lit = ring_point(s.pos, fraction);
            candidates.push(("ring", lit, 1, rgb(RING_COLOUR)));
        }
    }
    for b in &bd.buses {
        candidates.push(("bus", b.pos + BODY, 3, rgb(bus_colour(b.route))));
    }
    for r in &rd.routes {
        for pair in r.points.windows(2) {
            for k in 1..10 {
                let f = k as f32 / 10.0;
                candidates.push((
                    "route",
                    pair[0] + (pair[1] - pair[0]) * f,
                    0,
                    rgb(route_colour(r.id)),
                ));
            }
        }
    }
    for p in &sc.passengers {
        candidates.push(("passenger", p.pos, 2, rgb(PASSENGER_COLOUR)));
    }
    let centre = grid / 2.0;
    for d in [
        Vec2::new(10.0, 0.0),
        Vec2::new(-PANEL_HALF.x + 0.5, -PANEL_HALF.y + 0.5),
        Vec2::new(PANEL_HALF.x - 0.5, PANEL_HALF.y - 0.5),
    ] {
        candidates.push(("panel", centre + d, 3, rgb(PANEL_COLOUR)));
    }
    let (icon, first) = run_row(grid);
    candidates.push(("icon_run", icon, 3, rgb(DIGIT_COLOUR)));
    digit_candidates(&mut candidates, first, run.score);
    let (icon, first) = best_row(grid);
    candidates.push(("icon_best", icon, 3, rgb(BEST_COLOUR)));
    if let Some(b) = best {
        digit_candidates(&mut candidates, first, b);
    }
    for (x, y) in [(2.0, 2.0), (62.0, 38.0), (32.0, 2.0), (2.0, 38.0)] {
        candidates.push(("dot", Vec2::new(x, y), 0, rgb(DOT_COLOUR)));
        candidates.push(("clear", Vec2::new(x + 0.5, y + 0.5), usize::MAX, CLEAR));
    }
    let mut seen = BTreeSet::new();
    for (kind, grid, layer, colour) in candidates {
        let ok = match top(&layers, to_world(grid)) {
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
    let missing: Vec<_> = KINDS
        .iter()
        .filter(|k| !seen.contains(*k))
        .filter(|k| **k != "route" || !rd.routes.is_empty())
        .filter(|k| **k != "bus" || !bd.buses.is_empty())
        .collect();
    if missing.is_empty() {
        ExitCode::SUCCESS
    } else {
        eprintln!("missing sample kinds: {missing:?}");
        ExitCode::FAILURE
    }
}
