use proptest::prelude::*;
use salmong_core::ids::{Dir, RouteId, StationId};
use salmong_core::network::NetworkSnapshot;
use salmong_core::routing::{DijkstraRouter, HOP, INF, RouteTable, Router, TRANSFER, board_bit};
use salmong_core::scenario::Shape;
use salmong_oracle::router::ReferenceRouter;

const C: Shape = Shape::Circle;
const T: Shape = Shape::Triangle;
const S: Shape = Shape::Square;
const X: u32 = INF;

struct Expect<'a> {
    want: Shape,
    cost: &'a [u32],
    board: &'a [u16],
    ride: &'a [[&'a [u32]; 2]],
    stay: &'a [[&'a [bool]; 2]],
}

fn net(shapes: &[Shape], routes: &[&[u16]]) -> NetworkSnapshot {
    let routes = routes
        .iter()
        .map(|r| Some(r.iter().map(|&i| StationId(i)).collect()))
        .collect();
    NetworkSnapshot::new(42, shapes.to_vec(), routes).unwrap()
}

fn check(net: &NetworkSnapshot, expects: &[Expect]) {
    for table in [DijkstraRouter.solve(net), ReferenceRouter.solve(net)] {
        assert_eq!(table.rev(), net.rev());
        for e in expects {
            for (st, (&cost, &board)) in e.cost.iter().zip(e.board).enumerate() {
                let st = StationId(st as u16);
                assert_eq!(table.cost(e.want, st), cost, "cost {:?} {st:?}", e.want);
                assert_eq!(table.board(e.want, st), board, "board {:?} {st:?}", e.want);
            }
            for (r, (ride, stay)) in e.ride.iter().zip(e.stay).enumerate() {
                for dir in Dir::ALL {
                    let route = RouteId(r as u8);
                    assert_eq!(
                        table.ride(e.want, route, dir),
                        ride[dir.index()],
                        "ride {:?} r{r} {dir:?}",
                        e.want
                    );
                    assert_eq!(
                        table.stay(e.want, route, dir),
                        stay[dir.index()],
                        "stay {:?} r{r} {dir:?}",
                        e.want
                    );
                }
            }
        }
    }
}

#[test]
fn disconnected() {
    check(
        &net(&[C, T, S], &[&[1, 2]]),
        &[
            Expect {
                want: C,
                cost: &[0, X, X],
                board: &[0, 0, 0],
                ride: &[[&[X, X], &[X, X]]],
                stay: &[[&[true, false], &[true, false]]],
            },
            Expect {
                want: T,
                cost: &[X, 0, 40],
                board: &[0, 0, 0b10],
                ride: &[[&[0, 40], &[10, 0]]],
                stay: &[[&[false, false], &[true, false]]],
            },
        ],
    );
}

#[test]
fn shared_terminal() {
    check(
        &net(&[C, T, S], &[&[0, 1], &[2, 1]]),
        &[Expect {
            want: C,
            cost: &[0, 40, 80],
            board: &[0, 0b10, 0b100],
            ride: &[[&[0, 40], &[10, 0]], [&[50, 40], &[40, 80]]],
            stay: &[
                [&[false, false], &[true, false]],
                [&[true, false], &[false, false]],
            ],
        }],
    );
}

#[test]
fn parallel_routes() {
    let route: [&[u32]; 2] = [&[0, 40, 50], &[20, 10, 0]];
    let stay: [&[bool]; 2] = [&[false, false, false], &[true, true, false]];
    check(
        &net(&[C, T, S], &[&[0, 1, 2], &[0, 1, 2]]),
        &[Expect {
            want: C,
            cost: &[0, 40, 50],
            board: &[0, 0b1010, 0b1010],
            ride: &[route, route],
            stay: &[stay, stay],
        }],
    );
}

#[test]
fn reversal_and_no_path() {
    check(
        &net(&[T, C, S], &[&[0, 1, 2]]),
        &[
            Expect {
                want: S,
                cost: &[50, 40, 0],
                board: &[1, 1, 0],
                ride: &[[&[20, 10, 0], &[0, 40, 50]]],
                stay: &[[&[true, true, false], &[false, false, false]]],
            },
            Expect {
                want: Shape::Star,
                cost: &[X, X, X],
                board: &[0, 0, 0],
                ride: &[[&[X, X, X], &[X, X, X]]],
                stay: &[[&[true, true, false], &[true, true, false]]],
            },
        ],
    );
}

fn networks() -> impl Strategy<Value = NetworkSnapshot> {
    (2usize..=12)
        .prop_flat_map(|n| {
            let shapes = prop::collection::vec(0u8..4, n);
            let route = prop_oneof![
                1 => Just(None),
                4 => Just(()).prop_perturb(move |_, mut rng| {
                    let mut ids: Vec<u16> = (0..n as u16).collect();
                    for i in (1..ids.len()).rev() {
                        ids.swap(i, rng.random_range(0..=i));
                    }
                    ids.truncate(rng.random_range(2..=n));
                    Some(ids)
                }),
            ];
            (Just(n), shapes, prop::collection::vec(route, 0..=8))
        })
        .prop_map(|(_, shapes, routes)| {
            let shapes = shapes.into_iter().filter_map(Shape::from_index).collect();
            let routes = routes
                .into_iter()
                .map(|r| r.map(|ids| ids.into_iter().map(StationId).collect()))
                .collect();
            NetworkSnapshot::new(9, shapes, routes).unwrap()
        })
}

fn progress_holds(net: &NetworkSnapshot, table: &RouteTable) {
    for want in Shape::ALL {
        for slot in 0..net.route_slots() {
            let route = RouteId(slot as u8);
            let Some(stops) = net.stops(route) else {
                continue;
            };
            for dir in Dir::ALL {
                let ride = table.ride(want, route, dir);
                let stay = table.stay(want, route, dir);
                for i in 0..stops.len() {
                    let st = net.stop(route, dir, i).unwrap();
                    let cost = table.cost(want, st);
                    if stay[i] {
                        assert_eq!(ride[i], HOP.saturating_add(ride[i + 1]));
                    } else {
                        assert_eq!(ride[i], cost);
                    }
                    if table.board(want, st) & board_bit(route, dir) != 0 {
                        assert!(cost < INF);
                        assert_eq!(cost, TRANSFER + ride[i]);
                    }
                }
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]
    #[test]
    fn routers_agree(net in networks()) {
        let fast = DijkstraRouter.solve(&net);
        prop_assert_eq!(&fast, &ReferenceRouter.solve(&net));
        progress_holds(&net, &fast);
    }
}
