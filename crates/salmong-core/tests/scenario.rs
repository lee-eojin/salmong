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
