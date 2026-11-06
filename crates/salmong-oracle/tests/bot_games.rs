use salmong_core::scenario::Scenario;
use salmong_oracle::bots::{GAME_TICKS, GameResult, Greedy, Idle, play};
use salmong_oracle::generate_pack;

const SCENARIOS: usize = 16;

fn start_stations_only(scenario: &Scenario) -> Scenario {
    let mut scenario = scenario.clone();
    scenario.stations.retain(|s| s.at_tick == 0);
    scenario
}

fn games() -> Vec<(u32, GameResult, GameResult)> {
    let pack = generate_pack(1, 16).expect("seed 1 count 16 generates");
    pack.scenarios
        .iter()
        .map(start_stations_only)
        .map(|s| {
            let seed = u64::from(s.seed);
            (s.id, play(&s, seed, &mut Idle), play(&s, seed, &mut Greedy))
        })
        .collect()
}

#[test]
fn greedy_delivers_half_and_outlives_idle_on_all_sixteen() {
    assert_eq!(GAME_TICKS, 10_000);
    let games = games();
    assert_eq!(games.len(), SCENARIOS);
    assert_eq!(
        games.iter().map(|g| g.0).collect::<Vec<_>>(),
        (0..SCENARIOS as u32).collect::<Vec<_>>()
    );
    println!(
        "id | idle spawned delivered survived over | greedy spawned delivered survived over cmds"
    );
    for (id, idle, greedy) in &games {
        println!(
            "{id:2} | {:5} {:5} {:5} {:5} | {:5} {:5} {:5} {:5} {:3}",
            idle.spawned,
            idle.delivered,
            idle.survived,
            idle.over,
            greedy.spawned,
            greedy.delivered,
            greedy.survived,
            greedy.over,
            greedy.commands
        );
    }
    for (id, idle, greedy) in &games {
        for g in [idle, greedy] {
            assert!(
                g.over || g.survived == GAME_TICKS,
                "scenario {id} stopped early: {g:?}"
            );
            assert!(
                g.survived <= GAME_TICKS,
                "scenario {id} ran past the cap: {g:?}"
            );
            assert!(g.spawned > 0, "scenario {id}");
            assert_eq!(
                g.spawned,
                g.delivered + g.alive,
                "scenario {id} conservation"
            );
        }
        assert_eq!(idle.commands, 0, "scenario {id}");
        assert_eq!(idle.delivered, 0, "scenario {id}");
        assert!(greedy.commands > 0, "scenario {id}");
        assert!(
            greedy.delivered_ratio_at_least(1, 2),
            "scenario {id} greedy delivered {} of {}",
            greedy.delivered,
            greedy.spawned
        );
        assert!(
            greedy.outlives(idle),
            "scenario {id} greedy {greedy:?} idle {idle:?}"
        );
    }
}

#[test]
fn predicates_reject_counterexamples() {
    let r = |spawned, delivered, survived, over| GameResult {
        spawned,
        delivered,
        survived,
        over,
        commands: 0,
        alive: 0,
    };
    assert!(r(10, 5, 0, false).delivered_ratio_at_least(1, 2));
    assert!(!r(10, 4, 0, false).delivered_ratio_at_least(1, 2));
    assert!(!r(0, 0, 0, false).delivered_ratio_at_least(1, 2));
    let base = r(0, 0, 3000, true);
    assert!(r(0, 0, 3001, true).outlives(&base));
    assert!(!r(0, 0, 3000, true).outlives(&base));
    assert!(!r(0, 0, 2999, false).outlives(&r(0, 0, 3000, false)));
    assert!(r(0, 0, 10_000, false).outlives(&r(0, 0, 10_000, true)));
}

#[test]
fn games_are_deterministic() {
    assert_eq!(games(), games());
}
