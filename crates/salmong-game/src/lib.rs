pub mod best;
pub mod bus;
pub mod clock;
pub mod demo;
pub mod edits;
pub mod input;
pub mod pack;
pub mod render;
pub mod sim;
pub mod stress;

use bevy::app::PluginGroupBuilder;
use bevy::prelude::*;

const CLEAR_COLOUR: Color = Color::srgb_u8(0xFA, 0x80, 0x72);

pub struct BlankPlugin;

impl Plugin for BlankPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(CLEAR_COLOUR))
            .add_systems(Startup, spawn_camera);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

pub fn plugins() -> PluginGroupBuilder {
    DefaultPlugins
        .set(WindowPlugin {
            primary_window: Some(Window {
                fit_canvas_to_parent: true,
                ..default()
            }),
            ..default()
        })
        .add(BlankPlugin)
        .add(input::InputPlugin)
        .add(sim::SimPlugin)
        .add(clock::ClockPlugin)
        .add(bus::BusPlugin)
        .add(render::RenderPlugin)
        .add(demo::DemoPlugin)
        .add(edits::EditsPlugin)
        .add(stress::StressPlugin)
        .add(pack::PackPlugin)
        .add(best::BestPlugin)
}
