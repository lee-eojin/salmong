#![cfg(not(target_arch = "wasm32"))]

use bevy::prelude::*;
use salmong_core::balance::Balance;
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::score::FinalScore;
use salmong_core::sim::Sim;
use salmong_game::best::{BestPlugin, BestRes, BestStore};
use salmong_game::input::SimRes;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

struct Dir(PathBuf);

impl Dir {
    fn new() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("salmong-best-{}-{id}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        Self(dir)
    }

    fn file(&self) -> PathBuf {
        self.0.join("deep/er/best")
    }

    fn seed(&self, text: &str) {
        fs::create_dir_all(self.file().parent().expect("parent")).expect("parent dir");
        fs::write(self.file(), text).expect("seed");
    }

    fn read(&self) -> Option<String> {
        fs::read_to_string(self.file()).ok()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn scenario() -> Scenario {
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate: 1 }],
        stations: [
            (12, 7, Shape::Circle),
            (30, 22, Shape::Triangle),
            (48, 12, Shape::Square),
        ]
        .into_iter()
        .map(|(x, y, shape)| StationSpec {
            x,
            y,
            shape,
            at_tick: 0,
            weight: 3,
        })
        .collect(),
    }
}

fn balance() -> Balance {
    Balance {
        spawn_unit: 1,
        station_capacity: 0,
        overflow_limit: 3,
        ..Balance::mvp()
    }
}

const RUN: FinalScore = FinalScore { score: 0, ticks: 3 };

fn app_at(path: PathBuf) -> App {
    let mut app = App::new();
    app.insert_resource(BestStore::at(path));
    app.add_plugins(BestPlugin);
    app.update();
    app
}

fn start(app: &mut App) {
    app.insert_resource(SimRes {
        sim: Sim::new(scenario(), 1, balance()),
        scenario: scenario(),
    });
}

fn finish(app: &mut App) {
    let mut events = Vec::new();
    for _ in 0..3 {
        app.world_mut()
            .resource_mut::<SimRes>()
            .sim
            .step(&mut events);
    }
    app.update();
}

fn best(app: &App) -> Option<FinalScore> {
    app.world().resource::<BestRes>().0
}

#[test]
fn nothing_is_written_before_game_over() {
    let dir = Dir::new();
    let mut app = app_at(dir.file());
    assert_eq!(best(&app), None);
    start(&mut app);
    app.update();
    assert_eq!(best(&app), None);
    assert_eq!(dir.read(), None);
}

#[test]
fn game_over_writes_the_exact_record_and_leaves_no_scratch_file() {
    let dir = Dir::new();
    let mut app = app_at(dir.file());
    start(&mut app);
    finish(&mut app);
    assert_eq!(best(&app), Some(RUN));
    assert_eq!(dir.read().as_deref(), Some("salmong.best.1 0 3"));
    assert!(!dir.file().with_extension("tmp").exists());
}

#[test]
fn a_new_app_loads_the_record() {
    let dir = Dir::new();
    let mut first = app_at(dir.file());
    start(&mut first);
    finish(&mut first);
    let second = app_at(dir.file());
    assert_eq!(best(&second), Some(RUN));
}

#[test]
fn a_finished_game_writes_once() {
    let dir = Dir::new();
    let mut app = app_at(dir.file());
    start(&mut app);
    finish(&mut app);
    fs::remove_file(dir.file()).expect("remove");
    app.update();
    app.update();
    assert_eq!(dir.read(), None);
}

#[test]
fn a_better_record_survives_a_worse_run() {
    let dir = Dir::new();
    dir.seed("salmong.best.1 10 500");
    let mut app = app_at(dir.file());
    let held = FinalScore {
        score: 10,
        ticks: 500,
    };
    assert_eq!(best(&app), Some(held));
    start(&mut app);
    finish(&mut app);
    assert_eq!(best(&app), Some(held));
    assert_eq!(dir.read().as_deref(), Some("salmong.best.1 10 500"));
}

#[test]
fn a_worse_record_is_replaced() {
    let dir = Dir::new();
    dir.seed("salmong.best.1 0 2");
    let mut app = app_at(dir.file());
    start(&mut app);
    finish(&mut app);
    assert_eq!(best(&app), Some(RUN));
    assert_eq!(dir.read().as_deref(), Some("salmong.best.1 0 3"));
}

#[test]
fn a_corrupt_record_reads_as_none_and_is_replaced() {
    let dir = Dir::new();
    dir.seed("salmong.best.1 010 500");
    let mut app = app_at(dir.file());
    assert_eq!(best(&app), None);
    start(&mut app);
    finish(&mut app);
    assert_eq!(best(&app), Some(RUN));
    assert_eq!(dir.read().as_deref(), Some("salmong.best.1 0 3"));
}

#[test]
fn a_record_from_another_tab_is_not_lowered() {
    let dir = Dir::new();
    dir.seed("salmong.best.1 0 1");
    let mut app = app_at(dir.file());
    dir.seed("salmong.best.1 50 900");
    start(&mut app);
    finish(&mut app);
    assert_eq!(dir.read().as_deref(), Some("salmong.best.1 50 900"));
    assert_eq!(
        best(&app),
        Some(FinalScore {
            score: 50,
            ticks: 900
        })
    );
}

#[test]
fn an_unwritable_store_keeps_the_session_best() {
    let dir = Dir::new();
    dir.seed("not a directory");
    let mut app = app_at(dir.file().join("best"));
    start(&mut app);
    finish(&mut app);
    assert_eq!(best(&app), Some(RUN));
    assert_eq!(dir.read().as_deref(), Some("not a directory"));
}

#[test]
fn recording_leaves_the_sim_alone() {
    let dir = Dir::new();
    let mut app = app_at(dir.file());
    start(&mut app);
    finish(&mut app);
    let mut sim = Sim::new(scenario(), 1, balance());
    let mut events = Vec::new();
    for _ in 0..3 {
        sim.step(&mut events);
    }
    assert_eq!(
        app.world().resource::<SimRes>().sim.state_hash(),
        sim.state_hash()
    );
}
