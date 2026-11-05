use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseButton, MouseButtonInput};
use bevy::prelude::*;
use bevy::window::{PrimaryWindow, WindowEvent};
use salmong_core::ids::Tick;
use salmong_core::input::{Fit, Pointer, PointerEvent, bind};
use salmong_core::scenario::Scenario;
use salmong_core::sim::{Command, Sim};

#[derive(Resource)]
pub struct SimRes {
    pub sim: Sim,
    pub scenario: Scenario,
}

#[derive(Resource, Default)]
pub struct PointerRes(pub Pointer);

#[derive(Resource, Default)]
pub struct PendingCommands(pub Vec<Command>);

#[derive(Resource, Default)]
pub struct CommandLog(pub Vec<(Tick, Command)>);

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PointerRes>()
            .init_resource::<PendingCommands>()
            .init_resource::<CommandLog>()
            .add_systems(Update, read_pointer.run_if(resource_exists::<SimRes>))
            .add_systems(
                FixedUpdate,
                apply_commands.run_if(resource_exists::<SimRes>),
            );
    }
}

pub fn pointer_event(ev: &WindowEvent, fit: Option<Fit>) -> Option<PointerEvent> {
    match ev {
        WindowEvent::CursorMoved(m) => {
            let fit = fit?;
            Some(PointerEvent::Move(
                fit.world((m.position.x as i32, m.position.y as i32)),
            ))
        }
        WindowEvent::MouseButtonInput(MouseButtonInput {
            button: MouseButton::Left,
            state,
            ..
        }) => Some(match state {
            ButtonState::Pressed => PointerEvent::Press,
            ButtonState::Released => PointerEvent::Release,
        }),
        WindowEvent::CursorLeft(_) => Some(PointerEvent::Cancel),
        WindowEvent::WindowFocused(f) if !f.focused => Some(PointerEvent::Cancel),
        WindowEvent::KeyboardInput(KeyboardInput {
            key_code: KeyCode::Escape,
            state: ButtonState::Pressed,
            ..
        }) => Some(PointerEvent::Cancel),
        _ => None,
    }
}

fn read_pointer(
    mut events: MessageReader<WindowEvent>,
    window: Query<&Window, With<PrimaryWindow>>,
    sim: Res<SimRes>,
    mut pointer: ResMut<PointerRes>,
    mut pending: ResMut<PendingCommands>,
) {
    let fit = window
        .single()
        .ok()
        .and_then(|w| Fit::new((w.width() as u32, w.height() as u32), &sim.scenario));
    let net = sim.sim.snapshot();
    for ev in events.read() {
        if let Some(pe) = pointer_event(ev, fit)
            && let Some(cmd) = pointer.0.apply(pe, &sim.scenario, &net)
        {
            pending.0.push(cmd);
        }
    }
}

pub fn apply_commands(
    mut sim: ResMut<SimRes>,
    mut pending: ResMut<PendingCommands>,
    mut log: ResMut<CommandLog>,
) {
    bind(&mut sim.sim, &mut pending.0, &mut log.0);
}
