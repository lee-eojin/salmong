use salmong_core::scenario::{RampPoint, Scenario, ScenarioError, Shape, StationSpec};

fn example() -> Scenario {
    Scenario {
        id: 0,
        seed: 3141592653,
        w: 64,
        h: 40,
        ramp: vec![
            RampPoint { tick: 0, rate: 40 },
            RampPoint { tick: 9000, rate: 90 },
        ],
        stations: [
            (12, 7, Shape::Circle, 3),
            (30, 22, Shape::Triangle, 5),
            (48, 12, Shape::Square, 4),
        ]
        .into_iter()
        .map(|(x, y, shape, weight)| StationSpec { x, y, shape, at_tick: 0, weight })
        .collect(),
    }
}

#[test]
fn shape_index_follows_the_plan() {
    let shapes = [Shape::Circle, Shape::Triangle, Shape::Square, Shape::Star];
    for (index, shape) in shapes.into_iter().enumerate() {
        assert_eq!(Shape::from_index(index as u8), Some(shape));
        assert_eq!(shape.index(), index as u8);
        assert_eq!(Shape::ALL[index], shape);
    }
    assert_eq!(Shape::from_index(4), None);
}

#[test]
fn plan_example_is_valid() {
    assert_eq!(example().validate(), Ok(()));
}

#[test]
fn no_stations_is_too_few() {
    let mut scenario = example();
    scenario.stations.clear();
    assert_eq!(scenario.validate(), Err(ScenarioError::TooFewStations { count: 0 }));
}

fn station(x: u16, y: u16, shape: Shape, at_tick: u32) -> StationSpec {
    StationSpec { x, y, shape, at_tick, weight: 1 }
}

#[test]
fn validate_reports_the_earlier_of_two_broken_rules() {
    use ScenarioError::*;
    let mut cases: Vec<(Scenario, ScenarioError)> = Vec::new();

    let mut scenario = example();
    scenario.ramp = vec![RampPoint { tick: 5, rate: 40 }, RampPoint { tick: 5, rate: 50 }];
    cases.push((scenario, RampNotFromZero { tick: 5 }));

    let mut scenario = example();
    scenario.ramp = vec![RampPoint { tick: 0, rate: 40 }, RampPoint { tick: 0, rate: 30 }];
    cases.push((scenario, RampNotAscending { index: 1 }));

    let mut scenario = example();
    scenario.ramp = vec![RampPoint { tick: 0, rate: 40 }, RampPoint { tick: 9000, rate: 30 }];
    scenario.stations.clear();
    cases.push((scenario, RampFalls { index: 1 }));

    let mut scenario = example();
    scenario.stations = (0..65u32).map(|i| station(i as u16, 0, Shape::Circle, 65 - i)).collect();
    cases.push((scenario, TooManyStations { count: 65 }));

    let mut scenario = example();
    scenario.stations[1].at_tick = 10;
    cases.push((scenario, SpawnOrder { index: 2 }));

    let mut scenario = example();
    scenario.stations[0].shape = Shape::Triangle;
    scenario.stations.push(station(12, 30, Shape::Circle, 0));
    cases.push((scenario, StartCount { count: 4 }));

    let mut scenario = example();
    scenario.stations[0].shape = Shape::Triangle;
    scenario.stations[2].x = 64;
    cases.push((scenario, StartShapes));

    let mut scenario = example();
    scenario.stations[1].x = 63;
    scenario.stations[1].y = 12;
    scenario.stations[2].x = 64;
    cases.push((scenario, OutOfBounds { index: 2 }));

    let mut scenario = example();
    scenario.stations.push(station(12, 8, Shape::Star, 100));
    cases.push((scenario, TooClose { first: 0, second: 3 }));

    for (scenario, expected) in cases {
        assert_eq!(scenario.validate(), Err(expected), "{scenario:?}");
    }
}
