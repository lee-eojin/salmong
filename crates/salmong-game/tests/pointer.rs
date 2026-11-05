#![cfg(not(target_arch = "wasm32"))]

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseButton, MouseButtonInput};
use bevy::prelude::*;
use bevy::window::{
    CursorLeft, CursorMoved, PrimaryWindow, WindowEvent, WindowFocused, WindowResolution,
};
use salmong_core::balance::Balance;
use salmong_core::ids::{StationId, Tick};
use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_core::sim::{Command, Sim};
use salmong_game::input::{CommandLog, InputPlugin, PendingCommands, SimRes};

const PX: [(f32, f32); 3] = [(420.0, 189.0), (906.0, 594.0), (1392.0, 324.0)];

fn scenario() -> Scenario {
    Scenario {
        id: 0,
        seed: 1,
        w: 64,
        h: 40,
        ramp: vec![RampPoint { tick: 0, rate: 0 }],
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
            weight: 1,
        })
        .collect(),
    }
}

fn app() -> (App, Entity) {
    let mut app = App::new();
    app.add_message::<WindowEvent>().add_plugins(InputPlugin);
    let window = app
        .world_mut()
        .spawn((
            Window {
                resolution: WindowResolution::new(1920, 1080),
                ..default()
            },
            PrimaryWindow,
        ))
        .id();
    let sc = scenario();
    app.insert_resource(SimRes {
        sim: Sim::new(sc.clone(), 1, Balance::mvp()),
        scenario: sc,
    });
    (app, window)
}

fn moved(window: Entity, i: usize) -> WindowEvent {
    WindowEvent::CursorMoved(CursorMoved {
        window,
        position: Vec2::from(PX[i]),
        delta: None,
    })
}

fn button(window: Entity, button: MouseButton, state: ButtonState) -> WindowEvent {
    WindowEvent::MouseButtonInput(MouseButtonInput {
        button,
        state,
        window,
    })
}

fn left(window: Entity, state: ButtonState) -> WindowEvent {
    button(window, MouseButton::Left, state)
}

fn frame(app: &mut App, events: Vec<WindowEvent>) -> Vec<(Tick, Command)> {
    for e in events {
        app.world_mut().write_message(e);
    }
    app.world_mut().run_schedule(Update);
    app.world_mut().run_schedule(FixedUpdate);
    let mut events = Vec::new();
    app.world_mut()
        .resource_mut::<SimRes>()
        .sim
        .step(&mut events);
    app.world().resource::<CommandLog>().0.clone()
}

fn add(stops: &[u16]) -> Command {
    Command::AddRoute {
        stops: stops.iter().map(|&s| StationId(s)).collect(),
    }
}

fn drag_with(w: Entity, cancel: Option<WindowEvent>) -> Vec<WindowEvent> {
    let mut out = vec![moved(w, 0), left(w, ButtonState::Pressed), moved(w, 1)];
    out.extend(cancel);
    out.push(left(w, ButtonState::Released));
    out
}

#[test]
fn left_drag_logs_add_route_at_tick_zero() {
    let (mut app, w) = app();
    assert_eq!(
        frame(&mut app, drag_with(w, None)),
        [(Tick(0), add(&[0, 1]))]
    );
    assert!(app.world().resource::<PendingCommands>().0.is_empty());
}

#[test]
fn command_is_bound_to_the_tick_it_runs_on() {
    let (mut app, w) = app();
    frame(&mut app, vec![]);
    frame(&mut app, vec![]);
    assert_eq!(
        frame(&mut app, drag_with(w, None)),
        [(Tick(2), add(&[0, 1]))]
    );
    assert_eq!(app.world().resource::<SimRes>().sim.view().route_count, 1);
}

#[test]
fn right_button_drag_logs_nothing() {
    let (mut app, w) = app();
    let events = vec![
        moved(w, 0),
        button(w, MouseButton::Right, ButtonState::Pressed),
        moved(w, 1),
        button(w, MouseButton::Right, ButtonState::Released),
    ];
    assert!(frame(&mut app, events).is_empty());
}

#[test]
fn cancels_log_nothing() {
    let cancels = |w: Entity| {
        [
            WindowEvent::KeyboardInput(KeyboardInput {
                key_code: KeyCode::Escape,
                logical_key: Key::Escape,
                state: ButtonState::Pressed,
                text: None,
                repeat: false,
                window: w,
            }),
            WindowEvent::CursorLeft(CursorLeft { window: w }),
            WindowEvent::WindowFocused(WindowFocused {
                window: w,
                focused: false,
            }),
        ]
    };
    for i in 0..3 {
        let (mut app, w) = app();
        let ev = cancels(w)[i].clone();
        assert!(
            frame(&mut app, drag_with(w, Some(ev))).is_empty(),
            "cancel {i}"
        );
    }
}

#[test]
fn focus_gain_and_escape_release_do_not_cancel() {
    let (mut app, w) = app();
    let keep = [
        WindowEvent::WindowFocused(WindowFocused {
            window: w,
            focused: true,
        }),
        WindowEvent::KeyboardInput(KeyboardInput {
            key_code: KeyCode::Escape,
            logical_key: Key::Escape,
            state: ButtonState::Released,
            text: None,
            repeat: false,
            window: w,
        }),
        WindowEvent::KeyboardInput(KeyboardInput {
            key_code: KeyCode::KeyA,
            logical_key: Key::Character("a".into()),
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: w,
        }),
    ];
    let mut events = vec![moved(w, 0), left(w, ButtonState::Pressed), moved(w, 1)];
    events.extend(keep);
    events.push(left(w, ButtonState::Released));
    assert_eq!(frame(&mut app, events), [(Tick(0), add(&[0, 1]))]);
}

#[test]
fn events_in_one_frame_keep_their_order() {
    let (mut app, w) = app();
    let events = vec![
        moved(w, 0),
        left(w, ButtonState::Pressed),
        moved(w, 1),
        left(w, ButtonState::Released),
        moved(w, 2),
    ];
    assert_eq!(frame(&mut app, events), [(Tick(0), add(&[0, 1]))]);
}

#[test]
fn no_sim_means_no_systems_run() {
    let mut app = App::new();
    app.add_message::<WindowEvent>().add_plugins(InputPlugin);
    app.world_mut().run_schedule(Update);
    app.world_mut().run_schedule(FixedUpdate);
    assert!(app.world().resource::<CommandLog>().0.is_empty());
}
