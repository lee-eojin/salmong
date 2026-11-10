use bevy::prelude::*;
use bevy::time::TimeSystems;
use bevy::window::{AppLifecycle, WindowEvent};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Speed {
    #[default]
    X1,
    X2,
}

impl Speed {
    pub fn ratio(self) -> f32 {
        match self {
            Speed::X1 => 1.0,
            Speed::X2 => 2.0,
        }
    }
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockCommand {
    Pause,
    Resume,
    SetSpeed(Speed),
}

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    paused: bool,
    occluded: bool,
    suspended: bool,
    speed: Speed,
}

impl Clock {
    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn hidden(&self) -> bool {
        self.occluded || self.suspended
    }

    pub fn halted(&self) -> bool {
        self.paused || self.hidden()
    }

    pub fn speed(&self) -> Speed {
        self.speed
    }
}

pub struct ClockPlugin;

impl Plugin for ClockPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Clock>()
            .add_message::<ClockCommand>()
            .add_systems(
                First,
                (
                    hold_clock.before(TimeSystems),
                    release_clock.after(TimeSystems),
                ),
            );
    }
}

fn hold_clock(
    mut commands: MessageReader<ClockCommand>,
    mut window: MessageReader<WindowEvent>,
    mut clock: ResMut<Clock>,
    mut virt: ResMut<Time<Virtual>>,
) {
    let mut hold = clock.halted();
    for cmd in commands.read() {
        match cmd {
            ClockCommand::Pause => clock.paused = true,
            ClockCommand::Resume => clock.paused = false,
            ClockCommand::SetSpeed(s) => clock.speed = *s,
        }
        hold |= clock.halted();
    }
    for ev in window.read() {
        match ev {
            WindowEvent::WindowOccluded(e) => clock.occluded = e.occluded,
            WindowEvent::AppLifecycle(l) => {
                clock.suspended = matches!(l, AppLifecycle::WillSuspend | AppLifecycle::Suspended)
            }
            _ => continue,
        }
        hold |= clock.halted();
    }
    virt.set_relative_speed(clock.speed.ratio());
    if hold {
        virt.pause();
    }
}

fn release_clock(
    clock: Res<Clock>,
    mut virt: ResMut<Time<Virtual>>,
    mut fixed: ResMut<Time<Fixed>>,
) {
    if !clock.halted() && virt.is_paused() {
        virt.unpause();
        let o = fixed.overstep();
        fixed.discard_overstep(o);
    }
}
