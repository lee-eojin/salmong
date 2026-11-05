use salmong_core::balance::Balance;
use salmong_core::ids::{RouteId, StationId, Tick};
use salmong_core::replay::{RULES_VERSION, Replay, ReplayError, ReplayHeader, hash_sequence, run};
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::Command;

fn empty3_scenario() -> Scenario {
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate: 0 }],
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

const PACK: [u8; 32] = [0x39; 32];

fn header(balance: Balance) -> ReplayHeader {
    ReplayHeader {
        rules_version: RULES_VERSION,
        pack_sha256: PACK,
        scenario_id: empty3_scenario().id,
        seed: 0,
        balance,
    }
}

fn good_replay(body: Vec<(Tick, Command)>) -> Replay {
    Replay {
        header: header(Balance::mvp()),
        body,
    }
}

#[test]
fn empty3_hashes_match_hand_table() {
    let hashes = hash_sequence(empty3_scenario(), 0, Balance::mvp(), &[], 2);
    assert_eq!(
        hashes,
        vec![
            14587225508827618376,
            9340962682666644249,
            7185562617216877458,
        ]
    );
}

#[test]
fn run_accepts_matching_header() {
    let scenario = empty3_scenario();
    let replay = good_replay(Vec::new());
    let hashes = run(scenario, &PACK, 0, Balance::mvp(), &replay, 2).unwrap();
    assert_eq!(hashes.len(), 3);
    assert_eq!(hashes[0], 14587225508827618376);
}

#[test]
fn wrong_rules_version_is_refused() {
    let scenario = empty3_scenario();
    let mut replay = good_replay(Vec::new());
    replay.header.rules_version = RULES_VERSION + 1;
    assert_eq!(
        run(scenario, &PACK, 0, Balance::mvp(), &replay, 1),
        Err(ReplayError::RulesVersion)
    );
}

#[test]
fn wrong_pack_sha_is_refused() {
    let scenario = empty3_scenario();
    let replay = good_replay(Vec::new());
    let mut bad = PACK;
    bad[0] ^= 1;
    assert_eq!(
        run(scenario, &bad, 0, Balance::mvp(), &replay, 1),
        Err(ReplayError::PackSha256)
    );
}

#[test]
fn wrong_scenario_id_is_refused() {
    let mut scenario = empty3_scenario();
    scenario.id = 9;
    let replay = good_replay(Vec::new());
    assert_eq!(
        run(scenario, &PACK, 0, Balance::mvp(), &replay, 1),
        Err(ReplayError::ScenarioId)
    );
}

#[test]
fn wrong_seed_is_refused() {
    let scenario = empty3_scenario();
    let replay = good_replay(Vec::new());
    assert_eq!(
        run(scenario, &PACK, 1, Balance::mvp(), &replay, 1),
        Err(ReplayError::Seed)
    );
}

#[test]
fn wrong_balance_field_is_refused() {
    let scenario = empty3_scenario();
    let cases = [
        Balance {
            travel_ticks: Balance::mvp().travel_ticks + 1,
            ..Balance::mvp()
        },
        Balance {
            start_buses: Balance::mvp().start_buses + 1,
            ..Balance::mvp()
        },
        Balance {
            capacity: Balance::mvp().capacity + 1,
            ..Balance::mvp()
        },
        Balance {
            service_ticks: Balance::mvp().service_ticks + 1,
            ..Balance::mvp()
        },
        Balance {
            spawn_unit: Balance::mvp().spawn_unit + 1,
            ..Balance::mvp()
        },
        Balance {
            max_alive: Balance::mvp().max_alive + 1,
            ..Balance::mvp()
        },
        Balance {
            station_capacity: Balance::mvp().station_capacity + 1,
            ..Balance::mvp()
        },
        Balance {
            overflow_limit: Balance::mvp().overflow_limit + 1,
            ..Balance::mvp()
        },
    ];
    for balance in cases {
        let mut replay = good_replay(Vec::new());
        replay.header.balance = balance;
        assert_eq!(
            run(scenario.clone(), &PACK, 0, Balance::mvp(), &replay, 1),
            Err(ReplayError::Balance),
            "{balance:?}"
        );
    }
}

#[test]
fn encode_decode_round_trips_header_and_body() {
    let body = vec![
        (
            Tick(0),
            Command::AddRoute {
                stops: vec![StationId(0), StationId(1)],
            },
        ),
        (
            Tick(3),
            Command::EditRoute {
                route: RouteId(0),
                stops: vec![StationId(0), StationId(2)],
            },
        ),
        (Tick(5), Command::RemoveRoute { route: RouteId(0) }),
    ];
    let replay = good_replay(body);
    let bytes = replay.encode();
    assert_eq!(Replay::decode(&bytes), Ok(replay));
}

#[test]
fn refused_replay_does_not_advance_hashes() {
    let scenario = empty3_scenario();
    let mut replay = good_replay(Vec::new());
    replay.header.seed = 99;
    assert!(run(scenario, &PACK, 0, Balance::mvp(), &replay, 2).is_err());
}

#[test]
fn hash_sequence_applies_body_commands_on_their_tick() {
    let early = vec![(
        Tick(0),
        Command::AddRoute {
            stops: vec![StationId(0), StationId(1)],
        },
    )];
    let late = vec![(
        Tick(1),
        Command::AddRoute {
            stops: vec![StationId(0), StationId(1)],
        },
    )];
    let none = hash_sequence(empty3_scenario(), 0, Balance::mvp(), &[], 2);
    let on_zero = hash_sequence(empty3_scenario(), 0, Balance::mvp(), &early, 2);
    let on_one = hash_sequence(empty3_scenario(), 0, Balance::mvp(), &late, 2);
    assert_ne!(on_zero[1], none[1]);
    assert_eq!(on_one[1], none[1]);
    assert_ne!(on_one[2], none[2]);
    assert_ne!(on_zero, on_one);
}

#[test]
fn hash_sequence_applies_every_command_on_the_same_tick() {
    let one = vec![(
        Tick(0),
        Command::AddRoute {
            stops: vec![StationId(0), StationId(1)],
        },
    )];
    let two = vec![
        (
            Tick(0),
            Command::AddRoute {
                stops: vec![StationId(0), StationId(1)],
            },
        ),
        (
            Tick(0),
            Command::AddRoute {
                stops: vec![StationId(1), StationId(2)],
            },
        ),
    ];
    let a = hash_sequence(empty3_scenario(), 0, Balance::mvp(), &one, 1);
    let b = hash_sequence(empty3_scenario(), 0, Balance::mvp(), &two, 1);
    assert_ne!(a, b);
}
