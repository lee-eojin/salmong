use crate::ids::{RouteId, StationId, Tick};
use crate::network::NetworkSnapshot;
use crate::scenario::Scenario;
use crate::sim::{Command, Sim, SimMode};

pub const MILLI: i64 = 1000;
pub const HIT_RADIUS: i64 = 1500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct World {
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerEvent {
    Move(World),
    Press,
    Release,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fit {
    scale: i64,
    off_x: i64,
    off_y: i64,
}

impl Fit {
    pub fn new(window: (u32, u32), scenario: &Scenario) -> Option<Fit> {
        let (w, h) = (i64::from(window.0), i64::from(window.1));
        let (gw, gh) = (i64::from(scenario.w), i64::from(scenario.h));
        let scale = (w * MILLI / gw).min(h * MILLI / gh);
        let off = |size: i64, grid: i64| (size - grid * scale / MILLI) / 2;
        (scale > 0).then_some(Fit {
            scale,
            off_x: off(w, gw),
            off_y: off(h, gh),
        })
    }

    pub fn world(self, px: (i32, i32)) -> World {
        let axis = |p: i32, off: i64| (i64::from(p) - off) * MILLI * MILLI / self.scale;
        World {
            x: axis(px.0, self.off_x),
            y: axis(px.1, self.off_y),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pointer {
    at: Option<World>,
    trail: Vec<StationId>,
}

impl Pointer {
    pub fn apply(
        &mut self,
        ev: PointerEvent,
        scenario: &Scenario,
        net: &NetworkSnapshot,
    ) -> Option<Command> {
        match ev {
            PointerEvent::Move(at) => {
                self.at = Some(at);
                self.follow(self.hit(scenario, net));
                None
            }
            PointerEvent::Press => {
                self.trail.clear();
                self.trail.extend(self.hit(scenario, net));
                None
            }
            PointerEvent::Release => {
                let hit = self.hit(scenario, net);
                self.follow(hit);
                let trail = std::mem::take(&mut self.trail);
                if hit? == *trail.last()? {
                    trail_command(&trail, net)
                } else {
                    None
                }
            }
            PointerEvent::Cancel => {
                self.trail.clear();
                None
            }
        }
    }

    pub fn trail(&self) -> &[StationId] {
        &self.trail
    }

    fn hit(&self, scenario: &Scenario, net: &NetworkSnapshot) -> Option<StationId> {
        let at = self.at?;
        let gap = |p: i64, grid: u16| (p - i64::from(grid) * MILLI).pow(2);
        scenario
            .stations
            .iter()
            .take(net.shapes().len())
            .position(|s| gap(at.x, s.x) + gap(at.y, s.y) <= HIT_RADIUS.pow(2))
            .map(|i| StationId(i as u16))
    }

    fn follow(&mut self, hit: Option<StationId>) {
        let Some(hit) = hit.filter(|_| !self.trail.is_empty()) else {
            return;
        };
        if self.trail.iter().rev().nth(1) == Some(&hit) {
            self.trail.pop();
        } else if !self.trail.contains(&hit) {
            self.trail.push(hit);
        }
    }
}

enum Walk {
    Miss,
    Refuse,
    Stops(Vec<StationId>),
}

fn walk(trail: &[StationId], s: &[StationId]) -> Walk {
    let Some(i) = s.iter().position(|st| *st == trail[0]) else {
        return Walk::Miss;
    };
    let m = trail
        .iter()
        .zip(&s[i..])
        .take_while(|(a, b)| a == b)
        .count();
    let rest = &trail[m..];
    let on_route = |stations: &[StationId]| stations.iter().any(|st| s.contains(st));
    if rest.is_empty() {
        return Walk::Stops([&s[..i], &s[i + m - 1..]].concat());
    }
    if m >= 2 && i + m == s.len() {
        if on_route(rest) {
            return Walk::Refuse;
        }
        return Walk::Stops([s, rest].concat());
    }
    let inner = &trail[1..trail.len() - 1];
    if s.get(i + 1) == trail.last() && !on_route(inner) {
        return Walk::Stops([&s[..=i], inner, &s[i + 1..]].concat());
    }
    Walk::Miss
}

pub fn trail_command(trail: &[StationId], net: &NetworkSnapshot) -> Option<Command> {
    if trail.len() < 2 {
        return None;
    }
    for slot in 0..net.route_slots() {
        let route = RouteId(slot as u8);
        let Some(forward) = net.stops(route) else {
            continue;
        };
        let backward = forward.iter().rev().copied().collect::<Vec<_>>();
        for (s, reversed) in [(forward, false), (&backward, true)] {
            match walk(trail, s) {
                Walk::Miss => {}
                Walk::Refuse => return None,
                Walk::Stops(mut stops) => {
                    if reversed {
                        stops.reverse();
                    }
                    return Some(if stops.len() < 2 {
                        Command::RemoveRoute { route }
                    } else {
                        Command::EditRoute { route, stops }
                    });
                }
            }
        }
    }
    Some(Command::AddRoute {
        stops: trail.to_vec(),
    })
}

pub fn bind(sim: &mut Sim, pending: &mut Vec<Command>, log: &mut Vec<(Tick, Command)>) {
    if pending.is_empty() {
        return;
    }
    let view = sim.view();
    if view.mode == SimMode::Over {
        pending.clear();
        return;
    }
    for cmd in pending.drain(..) {
        sim.queue(cmd.clone());
        log.push((view.tick, cmd));
    }
}
