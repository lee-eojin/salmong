use salmong_core::scenario::RampPoint;
use salmong_core::spawn::rate;

#[test]
fn rate_interpolates_inside_a_segment_that_starts_after_tick_zero() {
    let ramp = [
        RampPoint { tick: 0, rate: 0 },
        RampPoint {
            tick: 100,
            rate: 10,
        },
        RampPoint {
            tick: 200,
            rate: 20,
        },
    ];
    assert_eq!(rate(&ramp, 150), 15);
    assert_eq!(rate(&ramp, 100), 10);
    assert_eq!(rate(&ramp, 250), 20);
}
