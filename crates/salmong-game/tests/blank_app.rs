#![cfg(not(target_arch = "wasm32"))]

use bevy::camera::ClearColorConfig;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::WgpuSettings;
use bevy::winit::WinitPlugin;
use salmong_core::balance::Balance;
use salmong_core::sim::Sim;
use salmong_game::best::BestRes;
use salmong_game::clock::Clock;
use salmong_game::game::Screen;
use salmong_game::input::SimRes;
use salmong_game::pack::ScenarioPack;
use salmong_game::{BlankPlugin, plugins};

fn blank_app() -> App {
    let mut app = App::new();
    app.add_plugins(BlankPlugin);
    app.update();
    app
}

#[test]
fn clear_colour_is_salmon() {
    let app = blank_app();
    let colour = app.world().get_resource::<ClearColor>();
    assert_eq!(
        colour.map(|c| c.0.to_srgba().to_u8_array()),
        Some([0xFA, 0x80, 0x72, 0xFF])
    );
}

#[test]
fn one_2d_camera_clears_with_the_global_colour() {
    let mut app = blank_app();
    let cameras: Vec<_> = app
        .world_mut()
        .query_filtered::<&Camera, With<Camera2d>>()
        .iter(app.world())
        .map(|camera| camera.clear_color.clone())
        .collect();
    assert!(
        matches!(cameras[..], [ClearColorConfig::Default]),
        "{cameras:?}"
    );
}

fn shipped_app() -> App {
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
    app
}

#[test]
fn shipped_plugins_start_without_a_window_or_gpu() {
    let app = shipped_app();
    assert_eq!(
        app.world()
            .resource::<ClearColor>()
            .0
            .to_srgba()
            .to_u8_array(),
        [0xFA, 0x80, 0x72, 0xFF]
    );
    assert!(app.world().contains_resource::<BestRes>());
    let t = app.world().resource::<Time<Fixed>>().timestep();
    assert_eq!(t, std::time::Duration::from_nanos(33_333_333));
    assert_eq!(app.world().resource::<Time<Virtual>>().max_delta(), t * 2);
    assert_eq!(*app.world().resource::<Clock>(), Clock::default());
}

#[test]
fn shipped_plugins_boot_scenario_zero_into_menu() {
    let app = shipped_app();
    let w = app.world();
    assert_eq!(*w.resource::<State<Screen>>().get(), Screen::Menu);
    let sim = w.resource::<SimRes>();
    assert_eq!(sim.scenario.id, 0);
    let sc = w.resource::<ScenarioPack>().0.scenarios[0].clone();
    let fresh = Sim::new(sc.clone(), u64::from(sc.seed), Balance::mvp());
    assert_eq!(sim.scenario, sc);
    assert_eq!(sim.sim.state_hash(), fresh.state_hash());
}
