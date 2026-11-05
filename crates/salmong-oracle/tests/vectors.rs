use salmong_core::scenario::{RampPoint, Shape, StationSpec};
use salmong_oracle::{generate_scenario, h32, pick, r};

#[test]
fn golden_h32() {
    let rows = [
        (0, 0, 0),
        (1, 0, 1364076727),
        (1, 1, 2980047484),
        (0xDEADBEEF, 0, 233162409),
        (42, 7, 142367074),
    ];
    for (s, i, expected) in rows {
        assert_eq!(h32(s, i), expected, "h32({s}, {i})");
    }
}

#[test]
fn golden_ramp_for_seed_one() {
    let scenario = generate_scenario(1, 0).unwrap();
    assert_eq!(
        scenario.ramp,
        [
            RampPoint { tick: 0, rate: 40 },
            RampPoint {
                tick: 9000,
                rate: 89,
            },
        ]
    );
    assert_eq!(pick(80, 100, r(scenario.seed, 5, 0, 0)), 89);
}

#[test]
fn golden_placement_for_seed_one() {
    let scenario = generate_scenario(1, 0).unwrap();
    assert_eq!(scenario.seed, 1364076727);
    assert_eq!(scenario.w, 64);
    assert_eq!(scenario.h, 40);
    let expected = [
        (35, 32, Shape::Circle, 0, 4),
        (13, 19, Shape::Triangle, 0, 4),
        (50, 33, Shape::Square, 0, 4),
        (10, 3, Shape::Square, 600, 3),
        (19, 28, Shape::Square, 1200, 5),
        (53, 24, Shape::Circle, 1800, 4),
        (5, 6, Shape::Circle, 2400, 5),
        (24, 21, Shape::Circle, 3000, 4),
    ];
    assert_eq!(scenario.stations.len(), 64);
    for (index, (x, y, shape, at_tick, weight)) in expected.into_iter().enumerate() {
        assert_eq!(
            scenario.stations[index],
            StationSpec {
                x,
                y,
                shape,
                at_tick,
                weight,
            },
            "station {index}"
        );
    }
}
