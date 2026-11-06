#![cfg(not(target_arch = "wasm32"))]

use std::fs;

use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::WgpuSettings;
use bevy::winit::WinitPlugin;
use salmong_core::balance::Balance;
use salmong_core::ids::Tick;
use salmong_core::sim::Sim;
use salmong_game::pack::ScenarioPack;
use salmong_game::plugins;
use salmong_pack::write_pack;

const ASSET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../assets/scenarios/pack.v1.json"
);

#[test]
fn shipped_plugins_load_the_asset_and_every_scenario_starts_a_sim() {
    let mut app = App::new();
    app.add_plugins(
        plugins().disable::<WinitPlugin>().set(RenderPlugin {
            render_creation: WgpuSettings {
                backends: None,
                ..default()
            }
            .into(),
            ..default()
        }),
    );
    app.finish();
    app.cleanup();
    app.update();
    let pack = &app.world().resource::<ScenarioPack>().0;
    assert!(
        write_pack(pack) == fs::read(ASSET).unwrap(),
        "loaded pack is not the asset on disk"
    );
    assert_eq!(
        (pack.generation, pack.pack_seed, pack.scenarios.len()),
        (1, 1, 16)
    );
    for scenario in &pack.scenarios {
        let mut sim = Sim::new(scenario.clone(), u64::from(scenario.seed), Balance::mvp());
        sim.step(&mut Vec::new());
        assert_eq!(sim.view().tick, Tick(1), "scenario {}", scenario.id);
    }
}
