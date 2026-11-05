#[path = "../tests/workload/mod.rs"]
mod workload;

use std::hint::black_box;
use std::time::{Duration, Instant};

use salmong_core::balance::Balance;
use salmong_core::network::NetworkSnapshot;
use salmong_core::routing::{DijkstraRouter, Router};
use workload::{Population, TICKS, WARMUP, log, run, scenario, stops};

const REPS: usize = 5;

fn main() {
    let balance = Balance::bench();
    let edits = log()
        .iter()
        .map(|(t, _)| t.0)
        .filter(|&t| t > 0)
        .collect::<Vec<_>>();
    let mut best = vec![Duration::MAX; TICKS as usize];
    let mut alive = vec![0; TICKS as usize];
    let mut hash = None;
    let mut pop = Population::default();
    for _ in 0..REPS {
        pop = Population::default();
        let sim = run(balance, |t, sim, events| {
            let start = Instant::now();
            sim.step(events);
            let took = start.elapsed();
            pop.observe(t, events, balance.max_alive);
            let i = t as usize;
            best[i] = best[i].min(took);
            alive[i] = pop.alive;
        });
        let h = sim.state_hash();
        assert_eq!(*hash.get_or_insert(h), h);
    }
    assert_eq!(pop.peak, balance.max_alive);
    assert!(pop.cap_tick.is_some_and(|t| t <= WARMUP));
    let peak = |edit: bool| {
        (1..TICKS as usize)
            .filter(|&t| edits.contains(&(t as u64)) == edit)
            .max_by_key(|&t| best[t])
            .map(|t| (t, best[t], alive[t]))
            .unwrap()
    };
    for (name, (t, d, a)) in [("plain", peak(false)), ("edit", peak(true))] {
        println!("step {name} peak {d:?} at tick {t} alive {a}");
    }
    let mut sorted = best.clone();
    sorted.sort();
    println!(
        "step p50 {:?} p99 {:?}",
        sorted[sorted.len() / 2],
        sorted[sorted.len() * 99 / 100]
    );
    let shapes = scenario().stations.iter().map(|s| s.shape).collect();
    let routes = (0..8).map(|r| Some(stops(r, true))).collect();
    let net = NetworkSnapshot::new(1, shapes, routes).unwrap();
    let mut solves = (0..2_000)
        .map(|_| {
            let start = Instant::now();
            black_box(DijkstraRouter.solve(black_box(&net)));
            start.elapsed()
        })
        .collect::<Vec<_>>();
    solves.sort();
    println!(
        "solve p50 {:?} max {:?}",
        solves[solves.len() / 2],
        solves[solves.len() - 1]
    );
    println!("peak alive {} cap tick {}", pop.peak, pop.cap_tick.unwrap());
}
