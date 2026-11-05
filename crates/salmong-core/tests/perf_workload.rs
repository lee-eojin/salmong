mod workload;

use std::sync::LazyLock;

use salmong_core::balance::Balance;
use salmong_core::scenario::Shape;
use salmong_core::sim::{SimEvent, SimMode};
use workload::{Population, TICKS, WARMUP, run};

struct Outcome {
    pop: Population,
    min_after_warmup: usize,
    rejected: usize,
    score_at_warmup: u64,
    hash: u64,
    view: salmong_core::sim::SimView,
}

fn play(balance: Balance) -> Outcome {
    let mut pop = Population::default();
    let mut min_after_warmup = usize::MAX;
    let mut rejected = 0;
    let mut score_at_warmup = 0;
    let sim = run(balance, |t, sim, events| {
        if t == WARMUP {
            score_at_warmup = sim.view().score;
        }
        sim.step(events);
        pop.observe(t, events, balance.max_alive);
        rejected += events
            .iter()
            .filter(|e| matches!(e, SimEvent::CommandRejected))
            .count();
        if t >= WARMUP {
            min_after_warmup = min_after_warmup.min(pop.alive);
        }
    });
    Outcome {
        pop,
        min_after_warmup,
        rejected,
        score_at_warmup,
        hash: sim.state_hash(),
        view: sim.view(),
    }
}

static BENCH: LazyLock<Outcome> = LazyLock::new(|| play(Balance::bench()));

#[test]
fn workload_reaches_cap_inside_warmup() {
    assert_eq!(BENCH.pop.peak, 5_000);
    assert!(BENCH.pop.cap_tick.is_some_and(|t| t <= WARMUP));
}

#[test]
fn workload_holds_near_cap_after_warmup() {
    assert!(
        BENCH.min_after_warmup >= 4_950,
        "{}",
        BENCH.min_after_warmup
    );
    let v = &BENCH.view;
    let riders = v.buses.iter().map(|b| b.riders.len()).sum::<usize>();
    assert_eq!(BENCH.pop.alive, v.queued + riders);
    assert_eq!(v.mode, SimMode::Running);
    assert_eq!(v.tick.0, TICKS);
    assert_eq!(BENCH.rejected, 0);
}

#[test]
fn workload_keeps_large_ineligible_queues() {
    let stars = BENCH
        .view
        .queues
        .iter()
        .flatten()
        .filter(|p| p.want == Shape::Star)
        .count();
    assert!(stars >= 2_500, "{stars}");
}

#[test]
fn workload_keeps_delivering_with_all_routes() {
    let v = &BENCH.view;
    assert!(v.score > BENCH.score_at_warmup);
    assert_eq!(v.route_count, 8);
    assert_eq!(v.buses.len(), 8);
    assert_eq!(v.free_buses, 24);
}

#[test]
fn workload_is_deterministic() {
    let again = play(Balance::bench());
    assert_eq!(again.hash, BENCH.hash);
    assert_eq!(again.pop.cap_tick, BENCH.pop.cap_tick);
}

#[test]
fn bench_values_are_load_bearing() {
    let mvp = play(Balance::mvp());
    assert_eq!(mvp.view.mode, SimMode::Over);
    assert!(mvp.pop.peak < 5_000);
    let b = Balance::bench();
    assert_eq!(b.max_alive, Balance::mvp().max_alive);
    assert_eq!(b.max_alive, 5_000);
    assert!(u64::from(b.overflow_limit) > TICKS);
    assert_eq!(
        b,
        Balance {
            travel_ticks: 1,
            start_buses: 32,
            service_ticks: 1,
            spawn_unit: 2_000,
            overflow_limit: u32::MAX,
            ..Balance::mvp()
        }
    );
}
