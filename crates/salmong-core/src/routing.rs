use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::ids::{Dir, RouteId, StationId};
use crate::network::NetworkSnapshot;
use crate::scenario::Shape;

pub const HOP: u32 = 10;
pub const TRANSFER: u32 = 30;
pub const INF: u32 = u32::MAX;

pub trait Router {
    fn solve(&self, net: &NetworkSnapshot) -> RouteTable;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapeTable {
    cost: Vec<u32>,
    board: Vec<u16>,
    ride: Vec<[Vec<u32>; 2]>,
    stay: Vec<[Vec<bool>; 2]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteTable {
    rev: u64,
    shapes: [ShapeTable; 4],
}

pub fn board_bit(route: RouteId, dir: Dir) -> u16 {
    1 << (2 * usize::from(route.0) + dir.index())
}

impl ShapeTable {
    pub fn derive(net: &NetworkSnapshot, cost: Vec<u32>, ride: Vec<[Vec<u32>; 2]>) -> Self {
        let mut board = vec![0u16; cost.len()];
        let mut stay: Vec<[Vec<bool>; 2]> = Vec::with_capacity(ride.len());
        for (slot, dirs) in ride.iter().enumerate() {
            let route = RouteId(slot as u8);
            let mut stay_dirs: [Vec<bool>; 2] = [Vec::new(), Vec::new()];
            for dir in Dir::ALL {
                let along = &dirs[dir.index()];
                for i in 0..along.len() {
                    let Some(st) = net.stop(route, dir, i) else {
                        continue;
                    };
                    let here = cost[usize::from(st.0)];
                    let next = along.get(i + 1).copied();
                    if next.is_some() && here < INF && TRANSFER.saturating_add(along[i]) == here {
                        board[usize::from(st.0)] |= board_bit(route, dir);
                    }
                    stay_dirs[dir.index()]
                        .push(next.is_some_and(|n| HOP.saturating_add(n) <= here));
                }
            }
            stay.push(stay_dirs);
        }
        Self {
            cost,
            board,
            ride,
            stay,
        }
    }
}

impl RouteTable {
    pub fn new(rev: u64, shapes: [ShapeTable; 4]) -> Self {
        Self { rev, shapes }
    }

    pub fn rev(&self) -> u64 {
        self.rev
    }

    pub fn cost(&self, want: Shape, st: StationId) -> u32 {
        self.shapes[usize::from(want.index())].cost[usize::from(st.0)]
    }

    pub fn board(&self, want: Shape, st: StationId) -> u16 {
        self.shapes[usize::from(want.index())].board[usize::from(st.0)]
    }

    pub fn ride(&self, want: Shape, route: RouteId, dir: Dir) -> &[u32] {
        &self.shapes[usize::from(want.index())].ride[usize::from(route.0)][dir.index()]
    }

    pub fn stay(&self, want: Shape, route: RouteId, dir: Dir) -> &[bool] {
        &self.shapes[usize::from(want.index())].stay[usize::from(route.0)][dir.index()]
    }
}

pub struct DijkstraRouter;

impl Router for DijkstraRouter {
    fn solve(&self, net: &NetworkSnapshot) -> RouteTable {
        RouteTable::new(net.rev(), Shape::ALL.map(|want| dijkstra(net, want)))
    }
}

fn dijkstra(net: &NetworkSnapshot, want: Shape) -> ShapeTable {
    let stations = net.shapes().len();
    let mut ride_base: Vec<[usize; 2]> = Vec::with_capacity(net.route_slots());
    let mut nodes = stations;
    for slot in 0..net.route_slots() {
        let len = net.stops(RouteId(slot as u8)).map_or(0, <[StationId]>::len);
        ride_base.push([nodes, nodes + len]);
        nodes += 2 * len;
    }
    let mut incoming: Vec<Vec<(usize, u32)>> = vec![Vec::new(); nodes];
    for (slot, base) in ride_base.iter().enumerate() {
        let route = RouteId(slot as u8);
        let Some(stops) = net.stops(route) else {
            continue;
        };
        for dir in Dir::ALL {
            for i in 0..stops.len() {
                let Some(st) = net.stop(route, dir, i) else {
                    continue;
                };
                let wait = usize::from(st.0);
                let here = base[dir.index()] + i;
                incoming[wait].push((here, 0));
                if i + 1 < stops.len() {
                    incoming[here].push((wait, TRANSFER));
                    incoming[here + 1].push((here, HOP));
                }
            }
        }
    }
    let mut dist = vec![INF; nodes];
    let mut heap = BinaryHeap::new();
    for (st, shape) in net.shapes().iter().enumerate() {
        if *shape == want {
            dist[st] = 0;
            heap.push(Reverse((0u32, st)));
        }
    }
    while let Some(Reverse((d, v))) = heap.pop() {
        if d > dist[v] {
            continue;
        }
        for &(u, w) in &incoming[v] {
            let nd = d.saturating_add(w);
            if nd < dist[u] {
                dist[u] = nd;
                heap.push(Reverse((nd, u)));
            }
        }
    }
    let ride = ride_base
        .iter()
        .enumerate()
        .map(|(slot, base)| {
            let len = net.stops(RouteId(slot as u8)).map_or(0, <[StationId]>::len);
            Dir::ALL.map(|dir| dist[base[dir.index()]..base[dir.index()] + len].to_vec())
        })
        .collect();
    ShapeTable::derive(net, dist[..stations].to_vec(), ride)
}
