use salmong_core::balance::Balance;
use salmong_core::ids::StationId;
use salmong_core::scenario::Scenario;
use salmong_core::sim::{Command, Sim, SimEvent, SimMode, SimView};

pub const GAME_TICKS: u64 = 10_000;

pub trait Bot {
    fn act(&mut self, view: &SimView) -> Vec<Command>;
}

pub struct Idle;

impl Bot for Idle {
    fn act(&mut self, _view: &SimView) -> Vec<Command> {
        Vec::new()
    }
}

pub struct Greedy;

impl Bot for Greedy {
    fn act(&mut self, view: &SimView) -> Vec<Command> {
        if view.free_buses == 0 || view.queued == 0 {
            return Vec::new();
        }
        let mut stops = (0..view.queues.len())
            .map(|s| StationId(s as u16))
            .collect::<Vec<_>>();
        stops.sort_by_key(|s| (std::cmp::Reverse(view.queues[usize::from(s.0)].len()), s.0));
        vec![Command::AddRoute { stops }]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameResult {
    pub spawned: u64,
    pub delivered: u64,
    pub survived: u64,
    pub over: bool,
    pub commands: u64,
    pub alive: u64,
}

impl GameResult {
    pub fn delivered_ratio_at_least(&self, num: u64, den: u64) -> bool {
        self.spawned > 0 && self.delivered * den >= self.spawned * num
    }

    pub fn outlives(&self, baseline: &GameResult) -> bool {
        self.survived > baseline.survived || (baseline.over && !self.over)
    }
}

pub fn play(scenario: &Scenario, seed: u64, bot: &mut dyn Bot) -> GameResult {
    let mut sim = Sim::new(scenario.clone(), seed, Balance::mvp());
    let mut events = Vec::new();
    let mut spawned = 0;
    let mut commands = 0;
    while sim.view().tick.0 < GAME_TICKS && sim.view().mode == SimMode::Running {
        for cmd in bot.act(&sim.view()) {
            commands += 1;
            sim.queue(cmd);
        }
        events.clear();
        sim.step(&mut events);
        spawned += events
            .iter()
            .filter(|e| matches!(e, SimEvent::Spawned { .. }))
            .count() as u64;
    }
    let view = sim.view();
    GameResult {
        spawned,
        delivered: view.score,
        survived: view.tick.0,
        over: view.mode == SimMode::Over,
        commands,
        alive: (view.queued + view.buses.iter().map(|b| b.riders.len()).sum::<usize>()) as u64,
    }
}
