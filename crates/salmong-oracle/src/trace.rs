use salmong_core::balance::Balance;
use salmong_core::ids::{Dir, PassengerId, RouteId, StationId, Tick};
use salmong_core::rng::Pcg32;
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{Command, Sim};
use salmong_core::trace::{Action, Phase, TraceState, TraceStep};

pub const TRACE_TICKS: u64 = 200;

pub const NETWORKS: [[&[u16]; 2]; 4] = [
    [&[0, 1, 2], &[2, 3]],
    [&[0, 1, 2], &[1, 3]],
    [&[0, 1], &[2, 3]],
    [&[2, 1, 0], &[2, 3]],
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corruption {
    Drop,
    Swap,
    Alter,
}

pub fn balance() -> Balance {
    Balance {
        travel_ticks: 3,
        start_buses: 2,
        capacity: 2,
        service_ticks: 2,
        spawn_unit: 600,
        max_alive: 6,
        station_capacity: 1000,
        overflow_limit: 1_000_000,
    }
}

fn scenario() -> Scenario {
    use Shape::*;
    let stations = [
        (0, 0, Circle, 2),
        (10, 0, Triangle, 1),
        (20, 10, Circle, 1),
        (30, 0, Square, 1),
    ]
    .into_iter()
    .map(|(x, y, shape, weight)| StationSpec {
        x,
        y,
        shape,
        at_tick: 0,
        weight,
    })
    .collect();
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![
            RampPoint { tick: 0, rate: 0 },
            RampPoint { tick: 1, rate: 100 },
        ],
        stations,
    }
}

fn stops(v: &[u16]) -> Vec<StationId> {
    v.iter().map(|&s| StationId(s)).collect()
}

pub fn sample(seed: u64) -> Vec<TraceStep> {
    let mut sim = Sim::new(scenario(), seed, balance());
    for route in NETWORKS[0] {
        sim.queue(Command::AddRoute {
            stops: stops(route),
        });
    }
    sim.step(&mut Vec::new());
    sim.start_trace();
    let mut rng = Pcg32::new(seed, 7);
    let mut current = 0;
    let mut removed = false;
    for tick in 1..TRACE_TICKS {
        if !removed && seed % 2 == 1 && tick == TRACE_TICKS * 3 / 4 {
            sim.queue(Command::RemoveRoute { route: RouteId(1) });
            removed = true;
        } else if !removed && rng.bounded(15) == 0 {
            let next = (current + 1 + rng.bounded(3) as usize) % NETWORKS.len();
            for (r, (want, have)) in NETWORKS[next].iter().zip(NETWORKS[current]).enumerate() {
                if *want != have {
                    sim.queue(Command::EditRoute {
                        route: RouteId(r as u8),
                        stops: stops(want),
                    });
                }
            }
            current = next;
        }
        sim.step(&mut Vec::new());
    }
    sim.take_trace()
}

pub fn corrupt(steps: &[TraceStep], kind: Corruption) -> Vec<TraceStep> {
    let Some(first) = steps.first().map(|s| s.before.clone()) else {
        return Vec::new();
    };
    let mut items = steps
        .iter()
        .map(|s| (s.tick, s.action, s.after.clone()))
        .collect::<Vec<_>>();
    let k = items.len() / 2;
    match kind {
        Corruption::Drop => {
            items.remove(k);
        }
        Corruption::Swap => items.swap(k, k + 1),
        Corruption::Alter => {
            let i = (k..items.len())
                .find(|&i| matches!(items[i].1, Action::Spawn { .. }))
                .unwrap_or(k);
            let bus = &mut items[i].2.buses[0];
            bus.dir = bus.dir.reverse();
        }
    }
    let mut before = first;
    items
        .into_iter()
        .map(|(tick, action, after)| {
            let step = TraceStep {
                tick,
                action,
                before: before.clone(),
                after: after.clone(),
            };
            before = after;
            step
        })
        .collect()
}

fn seq<T>(items: impl IntoIterator<Item = T>, f: impl Fn(T) -> String) -> String {
    format!(
        "<<{}>>",
        items.into_iter().map(f).collect::<Vec<_>>().join(", ")
    )
}

fn set<T>(items: impl IntoIterator<Item = T>, f: impl Fn(T) -> String) -> String {
    format!(
        "{{{}}}",
        items.into_iter().map(f).collect::<Vec<_>>().join(", ")
    )
}

fn station(s: &StationId) -> String {
    (s.0 + 1).to_string()
}

fn passenger(p: &PassengerId) -> String {
    (p.0 + 1).to_string()
}

fn shape(s: &Shape) -> String {
    format!("\"{s:?}\"")
}

fn phase(p: Phase) -> &'static str {
    match p {
        Phase::Moving => "\"Moving\"",
        Phase::Alighting => "\"Alighting\"",
        Phase::Boarding => "\"Boarding\"",
        Phase::Parked => "\"Parked\"",
    }
}

fn dir(d: Dir) -> &'static str {
    match d {
        Dir::Fwd => "\"Fwd\"",
        Dir::Back => "\"Back\"",
    }
}

fn state(s: &TraceState, passengers: usize) -> String {
    let unset = "\"Unset\"".to_string();
    let wants = (0..passengers).map(|i| s.wants.get(i).map_or(unset.clone(), shape));
    let fields = [
        ("network", seq(&s.network, |r| seq(r, station))),
        ("dir", seq(&s.buses, |b| dir(b.dir).to_string())),
        (
            "retiring",
            set((1..).zip(&s.buses).filter(|(_, b)| b.retiring), |(i, _)| {
                i.to_string()
            }),
        ),
        ("queue", seq(&s.queues, |q| seq(q, passenger))),
        ("onboard", seq(&s.buses, |b| seq(&b.riders, passenger))),
        ("busAt", seq(&s.buses, |b| station(&b.at))),
        ("busPhase", seq(&s.buses, |b| phase(b.phase).to_string())),
        ("delivered", set(&s.delivered, passenger)),
        ("edits", s.edits.to_string()),
        ("spawned", s.wants.len().to_string()),
        ("want", seq(wants, |w| w)),
    ];
    format!(
        "[{}]",
        fields
            .iter()
            .map(|(k, v)| format!("{k} |-> {v}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn label(a: Action) -> (&'static str, u16, u32) {
    match a {
        Action::Spawn { passenger, .. } => ("Spawn", 0, passenger.0 + 1),
        Action::EditNetwork => ("EditNetwork", 0, 0),
        Action::Move { bus } => ("Move", bus.0 + 1, 0),
        Action::Alight { bus, passenger } => ("Alight", bus.0 + 1, passenger.0 + 1),
        Action::EndAlight { bus } => ("EndAlight", bus.0 + 1, 0),
        Action::Board { bus, passenger } => ("Board", bus.0 + 1, passenger.0 + 1),
        Action::Depart { bus } => ("Depart", bus.0 + 1, 0),
    }
}

pub fn tla_module(steps: &[TraceStep], capacity: usize) -> String {
    let states = steps.iter().flat_map(|s| [&s.before, &s.after]);
    let passengers = states.clone().map(|s| s.wants.len()).max().unwrap_or(0);
    let edits = states.map(|s| s.edits).max().unwrap_or(0);
    let body = steps
        .iter()
        .map(|s| {
            let (act, bus, p) = label(s.action);
            let Tick(tick) = s.tick;
            format!(
                "  [tick |-> {tick}, act |-> \"{act}\", bus |-> {bus}, p |-> {p},\n   before |-> {},\n   after |-> {}]",
                state(&s.before, passengers),
                state(&s.after, passengers)
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        "---- MODULE BoardingTraceData ----\nEXTENDS Naturals, Sequences\nTraceCapacity == {capacity}\nTraceMaxPassengers == {passengers}\nTraceMaxEdits == {edits}\nTrace == <<\n{body}\n>>\n====\n"
    )
}
