#![cfg(not(target_arch = "wasm32"))]

use bevy::camera::ClearColorConfig;
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::WgpuSettings;
use bevy::winit::WinitPlugin;
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
    assert!(matches!(cameras[..], [ClearColorConfig::Default]), "{cameras:?}");
}

#[test]
fn shipped_plugins_start_without_a_window_or_gpu() {
    let mut app = App::new();
    app.add_plugins(plugins().disable::<WinitPlugin>().set(RenderPlugin {
        render_creation: WgpuSettings {
            backends: None,
            ..default()
        }
        .into(),
        ..default()
    }));
    app.finish();
    app.cleanup();
    app.update();
    assert_eq!(
        app.world()
            .resource::<ClearColor>()
            .0
            .to_srgba()
            .to_u8_array(),
        [0xFA, 0x80, 0x72, 0xFF]
    );
}
