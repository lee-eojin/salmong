use salmong_core::ids::{Dir, RouteId, StationId};
use salmong_core::network::{NetworkError, NetworkSnapshot};
use salmong_core::scenario::Shape;

fn stops(ids: &[u16]) -> Option<Vec<StationId>> {
    Some(ids.iter().map(|&i| StationId(i)).collect())
}

fn three() -> Vec<Shape> {
    vec![Shape::Circle, Shape::Triangle, Shape::Square]
}

#[test]
fn each_limit_has_its_error() {
    let cases = [
        (
            vec![Shape::Circle; 65],
            vec![],
            NetworkError::TooManyStations,
        ),
        (three(), vec![None; 9], NetworkError::TooManyRoutes),
        (
            three(),
            vec![None, stops(&[1])],
            NetworkError::RouteTooShort(RouteId(1)),
        ),
        (
            three(),
            vec![stops(&[0, 3])],
            NetworkError::UnknownStation(RouteId(0)),
        ),
        (
            three(),
            vec![stops(&[0, 1]), stops(&[0, 1, 0])],
            NetworkError::RepeatedStop(RouteId(1)),
        ),
    ];
    for (shapes, routes, want) in cases {
        assert_eq!(NetworkSnapshot::new(0, shapes, routes), Err(want));
    }
}

#[test]
fn limits_themselves_are_accepted() {
    let routes = (0..8).map(|_| stops(&[0, 63])).collect();
    assert!(NetworkSnapshot::new(0, vec![Shape::Circle; 64], routes).is_ok());
}

#[test]
fn back_reads_stops_in_reverse() {
    let net = NetworkSnapshot::new(7, three(), vec![None, stops(&[2, 0, 1])]).unwrap();
    let read = |dir| -> Vec<u16> {
        (0..4)
            .filter_map(|i| net.stop(RouteId(1), dir, i))
            .map(|s| s.0)
            .collect()
    };
    assert_eq!(read(Dir::Fwd), vec![2, 0, 1]);
    assert_eq!(read(Dir::Back), vec![1, 0, 2]);
    assert_eq!(net.stops(RouteId(0)), None);
    assert_eq!(net.rev(), 7);
}
