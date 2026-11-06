use salmong_core::balance::Balance;
use salmong_core::ids::{RouteId, StationId, Tick};
use salmong_core::network::NetworkSnapshot;
use salmong_core::scenario::{Scenario, StationSpec};
use salmong_core::score::FinalScore;
use salmong_core::sim::{Command, Sim, SimEvent, SimMode};
use salmong_oracle::bots::{Greedy, Idle, play};
use salmong_pack::parse_pack;

const PACK: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");
const SCENARIOS: usize = 16;
const TICK_CAP: u64 = 15_000;
const STARTERS: [&[u16]; 4] = [&[0, 1], &[1, 2], &[2, 0], &[0, 1, 2]];

struct Game {
    done: FinalScore,
    spawned: u64,
    rejected: u64,
    joins: Vec<(Tick, StationId)>,
    net: NetworkSnapshot,
}

fn scenarios() -> Vec<Scenario> {
    parse_pack(PACK).expect("shipped pack parses").scenarios
}

fn gap(stations: &[StationSpec], a: StationId, b: StationId) -> u32 {
    let (a, b) = (stations[usize::from(a.0)], stations[usize::from(b.0)]);
    u32::from(a.x.abs_diff(b.x)).pow(2) + u32::from(a.y.abs_diff(b.y)).pow(2)
}

fn link(net: &NetworkSnapshot, stations: &[StationSpec], new: StationId) -> Command {
    let (route, head) = (0..net.route_slots() as u8)
        .map(RouteId)
        .flat_map(|route| [(route, true), (route, false)])
        .min_by_key(|&(route, head)| {
            let stops = net.stops(route).expect("linker never removes a route");
            let end = if head {
                stops[0]
            } else {
                stops[stops.len() - 1]
            };
            (gap(stations, end, new), route.0, !head)
        })
        .expect("linker has routes");
    let mut stops = net.stops(route).expect("chosen route").to_vec();
    stops.insert(if head { 0 } else { stops.len() }, new);
    Command::EditRoute { route, stops }
}

fn link_every_station(scenario: &Scenario) -> Game {
    let mut sim = Sim::new(scenario.clone(), u64::from(scenario.seed), Balance::mvp());
    for stops in STARTERS {
        sim.queue(Command::AddRoute {
            stops: stops.iter().map(|&s| StationId(s)).collect(),
        });
    }
    let (mut spawned, mut rejected, mut joins) = (0, 0, Vec::new());
    let mut events = Vec::new();
    while sim.view().mode == SimMode::Running && sim.view().tick.0 < TICK_CAP {
        let tick = sim.view().tick;
        events.clear();
        sim.step(&mut events);
        for event in &events {
            match event {
                SimEvent::StationSpawned { station } => {
                    joins.push((tick, *station));
                    sim.queue(link(&sim.snapshot(), &scenario.stations, *station));
                }
                SimEvent::Spawned { .. } => spawned += 1,
                SimEvent::CommandRejected => rejected += 1,
                _ => {}
            }
        }
    }
    let done = sim
        .final_score()
        .unwrap_or_else(|| panic!("scenario {} not over at tick {TICK_CAP}", scenario.id));
    Game {
        done,
        spawned,
        rejected,
        joins,
        net: sim.snapshot(),
    }
}

fn expected_joins(scenario: &Scenario, ticks: u64) -> Vec<(Tick, StationId)> {
    scenario
        .stations
        .iter()
        .enumerate()
        .filter(|(_, s)| s.at_tick > 0 && u64::from(s.at_tick) < ticks)
        .map(|(k, s)| (Tick(u64::from(s.at_tick)), StationId(k as u16)))
        .collect()
}

fn on_a_route(net: &NetworkSnapshot, station: StationId) -> bool {
    (0..net.route_slots() as u8).any(|r| {
        net.stops(RouteId(r))
            .is_some_and(|stops| stops.contains(&station))
    })
}

#[test]
fn a_bot_that_links_every_station_reaches_game_over_on_all_sixteen() {
    let scenarios = scenarios();
    assert_eq!(scenarios.len(), SCENARIOS);
    println!("id | ticks score spawned stations");
    for scenario in &scenarios {
        let id = scenario.id;
        let game = link_every_station(scenario);
        let live = game.net.shapes().len();
        println!(
            "{id:2} | {:5} {:5} {:5} {:2}",
            game.done.ticks, game.done.score, game.spawned, live
        );
        assert_eq!(game.joins, expected_joins(scenario, game.done.ticks));
        assert_eq!(live, 3 + game.joins.len(), "scenario {id}");
        assert_eq!(game.rejected, 0, "scenario {id}");
        assert!(game.done.score * 2 >= game.spawned, "scenario {id}");
        let last = game.joins.last().expect("a station joined").0;
        for (tick, station) in &game.joins {
            assert!(
                *tick == last || on_a_route(&game.net, *station),
                "scenario {id} station {station:?} unlinked"
            );
        }
    }
}

#[test]
fn linking_outlives_doing_nothing_and_a_bot_that_links_nothing() {
    for scenario in &scenarios() {
        let seed = u64::from(scenario.seed);
        let idle = play(scenario, seed, &mut Idle);
        let greedy = play(scenario, seed, &mut Greedy);
        let linked = link_every_station(scenario).done;
        println!(
            "{:2} | idle {:5} {:5} | greedy {:5} {:5} {:5} | linked {:5} {:5}",
            scenario.id,
            idle.survived,
            idle.delivered,
            greedy.survived,
            greedy.delivered,
            greedy.spawned,
            linked.ticks,
            linked.score
        );
        assert!(idle.over && greedy.over, "scenario {}", scenario.id);
        assert_eq!(idle.delivered, 0, "scenario {}", scenario.id);
        assert!(greedy.survived > idle.survived, "scenario {}", scenario.id);
        assert!(linked.ticks > greedy.survived, "scenario {}", scenario.id);
        assert!(linked.score > greedy.delivered, "scenario {}", scenario.id);
    }
}

#[test]
fn games_repeat() {
    for scenario in &scenarios()[..2] {
        let a = link_every_station(scenario);
        let b = link_every_station(scenario);
        assert_eq!((a.done, a.joins), (b.done, b.joins));
    }
}
