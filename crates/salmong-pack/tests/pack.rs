use salmong_core::scenario::{RampPoint, Scenario, ScenarioError, Shape, StationSpec};
use salmong_pack::{Pack, PackError, parse_pack, write_pack};

const ONE: &str = "{\"schema\":\"salmong.scenario-pack\",\"version\":1,\"gen\":1,\"pack_seed\":1,\"scenarios\":[\n{\"id\":0,\"seed\":3141592653,\"w\":64,\"h\":40,\"ramp\":[[0,40],[9000,90]],\"stations\":[[12,7,0,0,3],[30,22,1,0,5],[48,12,2,0,4]]}\n]}\n";
const TWO: &str = "{\"schema\":\"salmong.scenario-pack\",\"version\":1,\"gen\":2,\"pack_seed\":9,\"scenarios\":[\n{\"id\":0,\"seed\":3141592653,\"w\":64,\"h\":40,\"ramp\":[[0,40],[9000,90]],\"stations\":[[12,7,0,0,3],[30,22,1,0,5],[48,12,2,0,4]]},\n{\"id\":1,\"seed\":7,\"w\":64,\"h\":40,\"ramp\":[[0,40]],\"stations\":[[2,2,2,0,3],[6,2,1,0,3],[10,2,0,0,3],[14,2,3,8400,5]]}\n]}\n";
const S: &str = "[[2,2,2,0,3],[6,2,1,0,3],[10,2,0,0,3],[14,2,3,8400,5]]";
fn mutate(from: &str, to: &str) -> String {
    assert_eq!(TWO.matches(from).count(), 1, "{from}");
    TWO.replace(from, to)
}
fn generated(n: usize) -> String {
    let rows = (0..n).map(|k| format!("[{},{},{},{},3]", 2+4*(k%8), 2+4*(k/8), if k<3 {k} else {k%3}, if k<3 {0} else {(k-2)*600})).collect::<Vec<_>>().join(",");
    format!("{{\"schema\":\"salmong.scenario-pack\",\"version\":1,\"gen\":1,\"pack_seed\":1,\"scenarios\":[\n{{\"id\":0,\"seed\":1,\"w\":64,\"h\":40,\"ramp\":[[0,40]],\"stations\":[{rows}]}}\n]}}\n")
}
fn hand_pack(two: bool) -> Pack {
    let mut scenarios = vec![Scenario {
        id: 0,
        seed: 3141592653,
        w: 64,
        h: 40,
        ramp: vec![
            RampPoint { tick: 0, rate: 40 },
            RampPoint { tick: 9000, rate: 90 },
        ],
        stations: [
            (12, 7, Shape::Circle, 0, 3),
            (30, 22, Shape::Triangle, 0, 5),
            (48, 12, Shape::Square, 0, 4),
        ]
        .into_iter()
        .map(|(x, y, shape, at_tick, weight)| StationSpec { x, y, shape, at_tick, weight })
        .collect(),
    }];
    if two {
        scenarios.push(Scenario {
            id: 1,
            seed: 7,
            w: 64,
            h: 40,
            ramp: vec![RampPoint { tick: 0, rate: 40 }],
            stations: [
                (2, 2, Shape::Square, 0, 3),
                (6, 2, Shape::Triangle, 0, 3),
                (10, 2, Shape::Circle, 0, 3),
                (14, 2, Shape::Star, 8400, 5),
            ]
            .into_iter()
            .map(|(x, y, shape, at_tick, weight)| StationSpec { x, y, shape, at_tick, weight })
            .collect(),
        });
    }
    Pack {
        generation: if two { 2 } else { 1 },
        pack_seed: if two { 9 } else { 1 },
        scenarios,
    }
}
#[test]
fn writes_the_plan_example() { assert_eq!(write_pack(&hand_pack(false)),ONE.as_bytes()); }
#[test]
fn writes_two_scenarios() { assert_eq!(write_pack(&hand_pack(true)),TWO.as_bytes()); }
#[test]
fn parses_exact_values() { assert_eq!(parse_pack(TWO.as_bytes()),Ok(hand_pack(true))); }
#[test]
fn parse_then_write_is_identity() {
    for input in [ONE.to_owned(),TWO.to_owned(),generated(64)] { assert_eq!(write_pack(&parse_pack(input.as_bytes()).unwrap()), input.as_bytes()); }
}

#[test]
fn rejects_json_errors() {
    let rows = [
        ("\"gen\":2,", "\"gen\":2,\"extra\":0,"),
        ("\"id\":1,", "\"id\":1,\"extra\":0,"),
        ("\"gen\":2,", ""),
        ("\"gen\":2,", "\"gen\":2,\"gen\":2,"),
        ("[14,2,3,8400,5]", "[14,2,3,8400]"),
        ("[14,2,3,8400,5]", "[14,2,3,8400,5,0]"),
        ("\"ramp\":[[0,40]],", "\"ramp\":[[0]],"),
        ("\"seed\":7", "\"seed\":-7"),
        ("\"seed\":7", "\"seed\":7.0"),
        ("\"seed\":7", "\"seed\":7e0"),
        ("\"seed\":7", "\"seed\":07"),
        ("\"seed\":7", "\"seed\":\"7\""),
        ("\"seed\":7", "\"seed\":4294967296"),
        ("[14,2,3,8400,5]", "[65536,2,3,8400,5]"),
    ];
    for (from,to) in rows { assert!(matches!(parse_pack(mutate(from,to).as_bytes()),Err(PackError::Json{..})), "{from}"); }
    for input in [String::new(), "{".into(), format!("{TWO}x")] { assert!(matches!(parse_pack(input.as_bytes()),Err(PackError::Json{..}))); }
}
#[test]
fn rejects_header_and_pack_errors() {
    let rows = [
        (mutate("\"version\":1", "\"version\":2"), PackError::Version { found: 2 }),
        (mutate("\"version\":1", "\"version\":0"), PackError::Version { found: 0 }),
        (mutate("\"schema\":\"salmong.scenario-pack\"", "\"schema\":\"salmong.other\""), PackError::Schema { found: "salmong.other".into() }),
        (mutate("[14,2,3,8400,5]", "[14,2,4,8400,5]"), PackError::UnknownShape { scenario: 1, station: 3, found: 4 }),
        (mutate("\"id\":1,", "\"id\":0,"), PackError::ScenarioId { index: 1, found: 0 }),
        (mutate("\"id\":1,", "\"id\":2,"), PackError::ScenarioId { index: 1, found: 2 }),
        (format!("{}\n]}}\n", ONE.lines().next().unwrap()), PackError::NoScenarios),
    ];
    for (input,error) in rows { assert_eq!(parse_pack(input.as_bytes()),Err(error)); }
}
#[test]
fn rejects_scenario_rules() {
    let rows = [
        ("\"ramp\":[[0,40]],", "\"ramp\":[],", ScenarioError::EmptyRamp),
        ("\"ramp\":[[0,40]],", "\"ramp\":[[1,40]],", ScenarioError::RampNotFromZero { tick: 1 }),
        ("\"ramp\":[[0,40]],", "\"ramp\":[[0,40],[0,50]],", ScenarioError::RampNotAscending { index: 1 }),
        ("\"ramp\":[[0,40]],", "\"ramp\":[[0,40],[9,50],[8,60]],", ScenarioError::RampNotAscending { index: 2 }),
        ("\"ramp\":[[0,40]],", "\"ramp\":[[0,40],[10,39]],", ScenarioError::RampFalls { index: 1 }),
        (S, "[]", ScenarioError::TooFewStations { count: 0 }),
        (S, "[[2,2,2,0,3],[6,2,1,0,3]]", ScenarioError::TooFewStations { count: 2 }),
        ("[14,2,3,8400,5]", "[14,2,0,600,5],[18,2,0,300,5]", ScenarioError::SpawnOrder { index: 4 }),
        ("[14,2,3,8400,5]", "[14,2,0,0,5]", ScenarioError::StartCount { count: 4 }),
        ("[10,2,0,0,3],[14,2,3,8400,5]", "[10,2,0,600,3],[14,2,3,8400,5]", ScenarioError::StartCount { count: 2 }),
        ("[2,2,2,0,3]", "[2,2,1,0,3]", ScenarioError::StartShapes),
        ("[2,2,2,0,3]", "[2,2,3,0,3]", ScenarioError::StartShapes),
        ("[14,2,3,8400,5]", "[64,2,3,8400,5]", ScenarioError::OutOfBounds { index: 3 }),
        ("[14,2,3,8400,5]", "[14,40,3,8400,5]", ScenarioError::OutOfBounds { index: 3 }),
        ("[14,2,3,8400,5]", "[13,4,3,8400,5]", ScenarioError::TooClose { first: 2, second: 3 }),
        ("[14,2,3,8400,5]", "[14,2,3,8399,5]", ScenarioError::EarlyStar { index: 3 }),
    ];
    for (from,to,error) in rows { assert_eq!(parse_pack(mutate(from,to).as_bytes()),Err(PackError::Scenario {index:1,error})); }
    assert_eq!(parse_pack(generated(65).as_bytes()),Err(PackError::Scenario {index:0,error:ScenarioError::TooManyStations{count:65}}));
}
#[test]
fn rejects_noncanonical_bytes() {
    let rows = [
        (mutate("\"salmong.scenario-pack\",", "\"salmong.scenario-pack\", "),Some(34)),
        (format!(" {TWO}"),Some(0)),
        (TWO[..TWO.len()-1].to_owned(),Some(TWO.len()-1)),
        (format!("{TWO}\n"),Some(TWO.len())),
        (mutate("\"w\":64,\"h\":40,\"ramp\":[[0,40]],", "\"h\":40,\"w\":64,\"ramp\":[[0,40]],"),None),
        (mutate(",\n{\"id\":1", ",\r\n{\"id\":1"),None),
        (mutate(",\n{\"id\":1", ",{\"id\":1"),None),
        (mutate("salmong.scenario-pack", r"salmong.scenario-pa\u0063k"),None),
    ];
    for (input,expected) in rows {
        let Err(PackError::NonCanonical{offset}) = parse_pack(input.as_bytes()) else { panic!("accepted {input}"); };
        if let Some(expected) = expected { assert_eq!(offset,expected); }
    }
}
#[test]
fn accepts_valid_boundaries() {
    let rows = [
        ("\"ramp\":[[0,40]],", "\"ramp\":[[0,40],[10,40]],"),
        (S, "[[2,2,2,0,3],[6,2,1,0,3],[10,2,0,0,3]]"),
        ("[14,2,3,8400,5]", "[14,2,0,600,5],[18,2,0,600,5]"),
        ("[14,2,3,8400,5]", "[63,39,3,8400,5]"),
        ("\"seed\":7", "\"seed\":4294967295"),
    ];
    let mut inputs = rows.into_iter().map(|(from,to)| mutate(from,to)).collect::<Vec<_>>();
    inputs.push(mutate(&format!("\"w\":64,\"h\":40,\"ramp\":[[0,40]],\"stations\":{S}"), "\"w\":65535,\"h\":65535,\"ramp\":[[0,40]],\"stations\":[[2,2,2,0,3],[6,2,1,0,3],[10,2,0,0,3],[65534,65534,3,8400,5]]"));
    for input in inputs { assert_eq!(write_pack(&parse_pack(input.as_bytes()).unwrap()),input.as_bytes()); }
}
