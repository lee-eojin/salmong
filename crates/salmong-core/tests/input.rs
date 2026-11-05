use salmong_core::balance::Balance;
use salmong_core::ids::{RouteId, StationId, Tick};
use salmong_core::input::PointerEvent::{Cancel, Move, Press, Release};
use salmong_core::input::{Fit, MILLI, Pointer, PointerEvent, World, bind, trail_command};
use salmong_core::network::NetworkSnapshot;
use salmong_core::replay::hash_sequence;
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{Command, Sim, SimEvent, SimMode};

const SEED: u64 = 1;
const OFF: PointerEvent = Move(World {
    x: 15_000,
    y: 15_000,
});
const STATIONS: [(u16, u16, Shape, u32); 7] = [
    (10, 10, Shape::Circle, 0),
    (20, 10, Shape::Triangle, 0),
    (30, 10, Shape::Square, 0),
    (40, 10, Shape::Circle, 0),
    (20, 20, Shape::Triangle, 0),
    (30, 20, Shape::Square, 0),
    (50, 30, Shape::Circle, 600),
];

type Log = Vec<(Tick, Command)>;

fn scenario(rate: u32) -> Scenario {
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate }],
        stations: STATIONS
            .into_iter()
            .map(|(x, y, shape, at_tick)| StationSpec {
                x,
                y,
                shape,
                at_tick,
                weight: 3,
            })
            .collect(),
    }
}

fn balance() -> Balance {
    Balance {
        start_buses: 8,
        ..Balance::mvp()
    }
}

fn ids(stops: &[u16]) -> Vec<StationId> {
    stops.iter().map(|&s| StationId(s)).collect()
}

fn add(stops: &[u16]) -> Command {
    Command::AddRoute { stops: ids(stops) }
}

fn edit(route: u8, stops: &[u16]) -> Command {
    Command::EditRoute {
        route: RouteId(route),
        stops: ids(stops),
    }
}

fn rm(route: u8) -> Command {
    Command::RemoveRoute {
        route: RouteId(route),
    }
}

fn at(station: u16) -> PointerEvent {
    let (x, y, ..) = STATIONS[usize::from(station)];
    Move(World {
        x: i64::from(x) * MILLI,
        y: i64::from(y) * MILLI,
    })
}

struct Rig {
    scenario: Scenario,
    sim: Sim,
    pointer: Pointer,
    pending: Vec<Command>,
    log: Log,
    events: Vec<SimEvent>,
}

impl Rig {
    fn new() -> Self {
        Self::with(0, balance())
    }

    fn with(rate: u32, balance: Balance) -> Self {
        Self {
            scenario: scenario(rate),
            sim: Sim::new(scenario(rate), SEED, balance),
            pointer: Pointer::default(),
            pending: Vec::new(),
            log: Vec::new(),
            events: Vec::new(),
        }
    }

    fn send(&mut self, events: &[PointerEvent]) {
        let net = self.sim.snapshot();
        for &ev in events {
            let cmd = self.pointer.apply(ev, &self.scenario, &net);
            self.pending.extend(cmd);
        }
    }

    fn drag(&mut self, trail: &[u16]) {
        self.send(&[at(trail[0]), Press]);
        for &station in &trail[1..] {
            self.send(&[at(station)]);
        }
        self.send(&[Release]);
    }

    fn tick(&mut self) {
        bind(&mut self.sim, &mut self.pending, &mut self.log);
        self.sim.step(&mut self.events);
    }

    fn rejected(&self) -> usize {
        self.events
            .iter()
            .filter(|e| **e == SimEvent::CommandRejected)
            .count()
    }
}

fn scripted(trails: &[&[u16]]) -> Rig {
    let mut rig = Rig::new();
    for trail in trails {
        rig.drag(trail);
        rig.tick();
    }
    assert_eq!(rig.rejected(), 0);
    rig
}

fn trails(script: &[&[u16]]) -> Log {
    scripted(script).log
}

fn raw(events: &[PointerEvent]) -> Log {
    let mut rig = Rig::new();
    rig.send(events);
    rig.tick();
    rig.log
}

fn ticked(cmds: &[Command]) -> Log {
    cmds.iter()
        .enumerate()
        .map(|(k, cmd)| (Tick(k as u64), cmd.clone()))
        .collect()
}

fn network(routes: &[&[u16]]) -> NetworkSnapshot {
    let shapes = STATIONS[..6].iter().map(|s| s.2).collect();
    let routes = routes.iter().map(|r| Some(ids(r))).collect();
    NetworkSnapshot::new(0, shapes, routes).expect("fixture network")
}

#[test]
fn case_01_trail_on_an_empty_network_adds_a_route() {
    assert_eq!(trails(&[&[0, 1, 2]]), ticked(&[add(&[0, 1, 2])]));
}

#[test]
fn case_02_walking_to_the_last_terminal_then_on_extends() {
    assert_eq!(
        trails(&[&[0, 1], &[0, 1, 2]]),
        ticked(&[add(&[0, 1]), edit(0, &[0, 1, 2])])
    );
}

#[test]
fn case_03_walking_to_the_first_terminal_then_on_keeps_list_order() {
    assert_eq!(
        trails(&[&[0, 1, 2], &[1, 0, 3]]),
        ticked(&[add(&[0, 1, 2]), edit(0, &[3, 0, 1, 2])])
    );
}

#[test]
fn case_04_walking_one_hop_to_the_last_terminal_then_on_extends() {
    assert_eq!(
        trails(&[&[0, 1, 2], &[1, 2, 4]]),
        ticked(&[add(&[0, 1, 2]), edit(0, &[0, 1, 2, 4])])
    );
}

#[test]
fn case_05_one_hop_off_a_terminal_is_a_new_route_so_a_triangle_forms() {
    assert_eq!(
        trails(&[&[0, 1], &[1, 2], &[0, 2]]),
        ticked(&[add(&[0, 1]), add(&[1, 2]), add(&[0, 2])])
    );
}

#[test]
fn case_06_detour_between_adjacent_stops_inserts() {
    assert_eq!(
        trails(&[&[0, 1, 2], &[1, 4, 2]]),
        ticked(&[add(&[0, 1, 2]), edit(0, &[0, 1, 4, 2])])
    );
}

#[test]
fn case_07_reversed_detour_inserts_in_list_order() {
    assert_eq!(
        trails(&[&[0, 1, 2], &[2, 4, 1]]),
        ticked(&[add(&[0, 1, 2]), edit(0, &[0, 1, 4, 2])])
    );
}

#[test]
fn case_08_detour_may_cross_several_new_stations() {
    assert_eq!(
        trails(&[&[0, 1], &[0, 4, 5, 1]]),
        ticked(&[add(&[0, 1]), edit(0, &[0, 4, 5, 1])])
    );
}

#[test]
fn case_09_walking_a_hop_removes_the_start_stop() {
    assert_eq!(
        trails(&[&[0, 1, 2], &[1, 2]]),
        ticked(&[add(&[0, 1, 2]), edit(0, &[0, 2])])
    );
}

#[test]
fn case_10_walking_a_hop_backward_removes_the_start_stop() {
    assert_eq!(
        trails(&[&[0, 1, 2], &[1, 0]]),
        ticked(&[add(&[0, 1, 2]), edit(0, &[0, 2])])
    );
}

#[test]
fn case_11_walking_from_a_terminal_removes_that_terminal() {
    assert_eq!(
        trails(&[&[0, 1, 2], &[0, 1]]),
        ticked(&[add(&[0, 1, 2]), edit(0, &[1, 2])])
    );
}

#[test]
fn case_12_backward_walk_keeps_list_order() {
    assert_eq!(
        trails(&[&[0, 1, 2, 3], &[2, 1]]),
        ticked(&[add(&[0, 1, 2, 3]), edit(0, &[0, 1, 3])])
    );
}

#[test]
fn case_13_long_walk_removes_start_and_crossed_stops() {
    assert_eq!(
        trails(&[&[0, 1, 2, 3], &[0, 1, 2]]),
        ticked(&[add(&[0, 1, 2, 3]), edit(0, &[2, 3])])
    );
}

#[test]
fn case_14_walking_a_whole_route_backward_removes_it() {
    assert_eq!(
        trails(&[&[0, 1, 2], &[2, 1, 0]]),
        ticked(&[add(&[0, 1, 2]), rm(0)])
    );
}

#[test]
fn case_15_walking_a_two_stop_route_removes_it() {
    assert_eq!(trails(&[&[0, 1], &[0, 1]]), ticked(&[add(&[0, 1]), rm(0)]));
}

#[test]
fn case_16_walking_separates_routes_that_share_stops() {
    assert_eq!(
        trails(&[&[0, 1], &[4, 0, 1], &[0, 1, 2], &[4, 0, 1, 3]]),
        ticked(&[
            add(&[0, 1]),
            add(&[4, 0, 1]),
            edit(0, &[0, 1, 2]),
            edit(1, &[4, 0, 1, 3]),
        ])
    );
}

#[test]
fn case_17_removed_slots_are_skipped() {
    assert_eq!(
        trails(&[&[0, 1], &[2, 3], &[0, 1], &[2, 3, 4]]),
        ticked(&[add(&[0, 1]), add(&[2, 3]), rm(0), edit(1, &[2, 3, 4])])
    );
}

#[test]
fn case_18_extension_that_returns_to_the_route_is_refused() {
    assert_eq!(
        trails(&[&[0, 1, 2], &[1, 2, 0]]),
        ticked(&[add(&[0, 1, 2])])
    );
}

#[test]
fn case_19_leaving_the_route_before_a_terminal_is_a_new_route() {
    assert_eq!(
        trails(&[&[0, 1, 2], &[0, 1, 4]]),
        ticked(&[add(&[0, 1, 2]), add(&[0, 1, 4])])
    );
}

#[test]
fn case_20_detour_through_the_route_is_a_new_route() {
    assert_eq!(
        trails(&[&[0, 1, 2], &[0, 2, 1]]),
        ticked(&[add(&[0, 1, 2]), add(&[0, 2, 1])])
    );
}

#[test]
fn case_21_drag_back_undoes() {
    assert_eq!(
        raw(&[at(0), Press, at(1), at(2), at(1), Release]),
        ticked(&[add(&[0, 1])])
    );
}

#[test]
fn case_22_returning_past_the_second_last_changes_nothing() {
    assert_eq!(
        raw(&[at(0), Press, at(1), at(2), at(0), at(2), Release]),
        ticked(&[add(&[0, 1, 2])])
    );
}

#[test]
fn case_23_release_on_an_earlier_trail_station_cancels() {
    assert_eq!(raw(&[at(0), Press, at(1), at(2), at(0), Release]), []);
}

#[test]
fn case_24_release_off_a_station_cancels() {
    assert_eq!(raw(&[at(0), Press, at(1), OFF, Release]), []);
}

#[test]
fn case_25_press_off_a_station_starts_nothing() {
    assert_eq!(raw(&[OFF, Press, at(0), at(1), Release]), []);
}

#[test]
fn case_26_click_forms_no_command() {
    assert_eq!(raw(&[at(0), Press, Release]), []);
}

#[test]
fn case_27_return_to_the_start_cancels() {
    assert_eq!(raw(&[at(0), Press, at(1), at(0), Release]), []);
}

#[test]
fn case_28_cancel_drops_the_trail() {
    let mut rig = Rig::new();
    rig.send(&[at(0), Press, at(1)]);
    assert_eq!(rig.pointer.trail(), ids(&[0, 1]));
    rig.send(&[Cancel]);
    assert_eq!(rig.pointer.trail(), []);
    rig.send(&[Release]);
    rig.tick();
    assert_eq!(rig.log, []);
}

#[test]
fn case_29_press_during_a_drag_restarts() {
    assert_eq!(
        raw(&[at(0), Press, at(1), Press, at(2), Release]),
        ticked(&[add(&[1, 2])])
    );
}

#[test]
fn case_30_press_before_any_move_starts_nothing() {
    assert_eq!(raw(&[Press, at(0), at(1), Release]), []);
}

#[test]
fn case_31_stations_not_yet_open_are_not_hit() {
    assert_eq!(raw(&[at(0), Press, at(6), Release]), []);
}

#[test]
fn case_32_commands_bind_to_the_tick_that_runs_them() {
    let mut rig = Rig::new();
    rig.tick();
    rig.tick();
    rig.tick();
    rig.drag(&[0, 1]);
    rig.tick();
    assert_eq!(rig.log, [(Tick(3), add(&[0, 1]))]);
    assert_eq!(rig.sim.view().route_count, 1);
}

#[test]
fn case_33_gesture_order_is_kept_within_a_tick() {
    let mut rig = Rig::new();
    rig.drag(&[0, 1]);
    rig.drag(&[2, 3]);
    rig.tick();
    assert_eq!(rig.log, [(Tick(0), add(&[0, 1])), (Tick(0), add(&[2, 3]))]);
}

#[test]
fn case_34_a_command_binds_once() {
    let mut rig = Rig::new();
    rig.drag(&[0, 1]);
    rig.tick();
    rig.tick();
    assert_eq!(rig.log, [(Tick(0), add(&[0, 1]))]);
    assert_eq!(rig.pending, []);
}

#[test]
fn case_35_sim_rejections_stay_in_the_log() {
    let mut rig = Rig::with(
        0,
        Balance {
            start_buses: 1,
            ..balance()
        },
    );
    rig.drag(&[0, 1]);
    rig.tick();
    rig.drag(&[2, 3]);
    rig.tick();
    assert_eq!(rig.log, [(Tick(0), add(&[0, 1])), (Tick(1), add(&[2, 3]))]);
    assert_eq!(rig.rejected(), 1);
    assert_eq!(rig.sim.view().route_count, 1);
}

#[test]
fn case_36_commands_after_game_over_are_dropped_unlogged() {
    let mut rig = Rig::with(
        1,
        Balance {
            spawn_unit: 1,
            station_capacity: 0,
            overflow_limit: 1,
            ..balance()
        },
    );
    rig.tick();
    assert_eq!(rig.sim.view().mode, SimMode::Over);
    let before = rig.sim.state_hash();
    rig.drag(&[0, 1]);
    assert_eq!(rig.pending, [add(&[0, 1])]);
    rig.tick();
    assert_eq!(rig.log, []);
    assert_eq!(rig.pending, []);
    assert_eq!(rig.sim.state_hash(), before);
}

#[test]
fn case_37_the_log_replays_to_the_live_hash() {
    let rig = scripted(&[&[0, 1], &[4, 0, 1], &[0, 1, 2], &[4, 0, 1, 3]]);
    let hashes = hash_sequence(scenario(0), SEED, balance(), &rig.log, 4);
    assert_eq!(hashes.last(), Some(&rig.sim.state_hash()));
}

#[test]
fn case_38_hit_radius_is_inclusive() {
    let press_near_station_1 = |dx: i64, dy: i64| {
        let mut rig = Rig::new();
        let at = World {
            x: 20_000 + dx,
            y: 10_000 + dy,
        };
        rig.send(&[Move(at), Press]);
        rig.pointer.trail().to_vec()
    };
    for (dx, dy) in [(1500, 0), (-1500, 0), (0, 1500), (0, -1500), (900, 1200)] {
        assert_eq!(press_near_station_1(dx, dy), ids(&[1]), "({dx},{dy})");
    }
    for (dx, dy) in [(1501, 0), (-1501, 0), (0, 1501), (0, -1501), (901, 1200)] {
        assert_eq!(press_near_station_1(dx, dy), [], "({dx},{dy})");
    }
}

#[test]
fn case_39_fit_letterboxes_the_grid() {
    let grid = scenario(0);
    let world = |window, px| Fit::new(window, &grid).expect("window fits").world(px);
    let point = |x, y| World { x, y };
    assert_eq!(world((1920, 1080), (420, 189)), point(12_000, 7_000));
    assert_eq!(world((1920, 1080), (0, 0)), point(-3_555, 0));
    assert_eq!(world((1280, 1024), (240, 252)), point(12_000, 7_000));
    assert_eq!(world((1366, 768), (683, 384)), point(31_979, 20_000));
    assert_eq!(Fit::new((0, 600), &grid), None);
    assert_eq!(Fit::new((800, 0), &grid), None);
}

#[test]
fn trails_under_two_form_no_command() {
    assert_eq!(trail_command(&[], &network(&[])), None);
    assert_eq!(trail_command(&ids(&[0]), &network(&[])), None);
    assert_eq!(trail_command(&ids(&[0]), &network(&[&[0, 1]])), None);
}

#[test]
fn reversed_extension_that_returns_to_the_route_is_refused() {
    assert_eq!(
        trail_command(&ids(&[1, 0, 2]), &network(&[&[0, 1, 2]])),
        None
    );
}

#[test]
fn a_refusal_ends_the_search() {
    let net = network(&[&[0, 1, 2], &[1, 2, 0]]);
    assert_eq!(trail_command(&ids(&[1, 2, 0]), &net), None);
}

#[test]
fn routes_are_tried_in_id_order() {
    let net = network(&[&[0, 1], &[0, 1, 2]]);
    assert_eq!(trail_command(&ids(&[0, 1]), &net), Some(rm(0)));
}

#[test]
fn release_applies_the_trail_rule() {
    let grid = scenario(0);
    let open = |count: usize| {
        let shapes = STATIONS[..count].iter().map(|s| s.2).collect();
        NetworkSnapshot::new(0, shapes, vec![]).expect("fixture network")
    };
    let (six, seven) = (open(6), open(7));
    let mut pointer = Pointer::default();
    for ev in [at(0), Press, at(1), at(6)] {
        assert_eq!(pointer.apply(ev, &grid, &six), None);
    }
    assert_eq!(pointer.trail(), ids(&[0, 1]));
    assert_eq!(pointer.apply(Release, &grid, &seven), Some(add(&[0, 1, 6])));
    assert_eq!(pointer.trail(), []);
}
