use salmong_core::ids::{Dir, RouteId};
use salmong_core::network::NetworkSnapshot;
use salmong_core::routing::{HOP, INF, RouteTable, Router, ShapeTable, TRANSFER};
use salmong_core::scenario::Shape;

pub struct ReferenceRouter;

impl Router for ReferenceRouter {
    fn solve(&self, net: &NetworkSnapshot) -> RouteTable {
        RouteTable::new(net.rev(), Shape::ALL.map(|want| bellman_ford(net, want)))
    }
}

fn bellman_ford(net: &NetworkSnapshot, want: Shape) -> ShapeTable {
    let shapes = net.shapes();
    let mut cost: Vec<u32> = shapes
        .iter()
        .map(|s| if *s == want { 0 } else { INF })
        .collect();
    let mut ride: Vec<[Vec<u32>; 2]> = (0..net.route_slots())
        .map(|slot| {
            let len = net.stops(RouteId(slot as u8)).map_or(0, <[_]>::len);
            [vec![INF; len], vec![INF; len]]
        })
        .collect();
    loop {
        let mut changed = false;
        for (slot, dirs) in ride.iter_mut().enumerate() {
            let route = RouteId(slot as u8);
            for dir in Dir::ALL {
                let along = &mut dirs[dir.index()];
                for i in (0..along.len()).rev() {
                    let Some(st) = net.stop(route, dir, i) else {
                        continue;
                    };
                    let here = cost[usize::from(st.0)];
                    let value = if shapes[usize::from(st.0)] == want {
                        0
                    } else if i + 1 == along.len() {
                        here
                    } else {
                        here.min(HOP.saturating_add(along[i + 1]))
                    };
                    if value != along[i] {
                        along[i] = value;
                        changed = true;
                    }
                }
            }
        }
        for (st, shape) in shapes.iter().enumerate() {
            if *shape == want {
                continue;
            }
            let mut best = INF;
            for (slot, dirs) in ride.iter().enumerate() {
                for dir in Dir::ALL {
                    let along = &dirs[dir.index()];
                    let boardable = along.len().saturating_sub(1);
                    for (i, value) in along.iter().enumerate().take(boardable) {
                        if net
                            .stop(RouteId(slot as u8), dir, i)
                            .map(|s| usize::from(s.0))
                            == Some(st)
                        {
                            best = best.min(TRANSFER.saturating_add(*value));
                        }
                    }
                }
            }
            if best != cost[st] {
                cost[st] = best;
                changed = true;
            }
        }
        if !changed {
            return ShapeTable::derive(net, cost, ride);
        }
    }
}
