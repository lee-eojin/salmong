use crate::game::{GameLog, Screen, emit};
use crate::sim::{ReplayFeed, SimEventMessage};
use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::window::WindowEvent;
use salmong_core::sim::SimEvent;
use std::time::Duration;

pub use device::MuteStore;

const CUES: usize = 6;
const BLIP_GAP: Duration = Duration::from_millis(90);
const MODIFIERS: [KeyCode; 6] = [
    KeyCode::ControlLeft,
    KeyCode::ControlRight,
    KeyCode::SuperLeft,
    KeyCode::SuperRight,
    KeyCode::AltLeft,
    KeyCode::AltRight,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Board,
    Deliver,
    Station,
    Overflow,
    Reject,
    Over,
}

#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CueMessage(pub Cue);

#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Muted(pub bool);

pub fn cue(ev: &SimEvent) -> Option<Cue> {
    match ev {
        SimEvent::Boarded { .. } => Some(Cue::Board),
        SimEvent::Delivered { .. } => Some(Cue::Deliver),
        SimEvent::StationSpawned { .. } => Some(Cue::Station),
        SimEvent::OverflowStarted { .. } => Some(Cue::Overflow),
        SimEvent::CommandRejected => Some(Cue::Reject),
        SimEvent::GameOver => Some(Cue::Over),
        SimEvent::Spawned { .. } | SimEvent::Alighted { .. } => None,
    }
}

pub fn wav(cue: Cue) -> &'static [u8] {
    match cue {
        Cue::Board => include_bytes!("../../../assets/sounds/board.wav"),
        Cue::Deliver => include_bytes!("../../../assets/sounds/deliver.wav"),
        Cue::Station => include_bytes!("../../../assets/sounds/station.wav"),
        Cue::Overflow => include_bytes!("../../../assets/sounds/overflow.wav"),
        Cue::Reject => include_bytes!("../../../assets/sounds/reject.wav"),
        Cue::Over => include_bytes!("../../../assets/sounds/over.wav"),
    }
}

pub fn volume(cue: Cue) -> f32 {
    match cue {
        Cue::Board => 0.35,
        Cue::Deliver => 0.6,
        Cue::Station => 0.7,
        Cue::Overflow => 0.8,
        Cue::Reject => 0.5,
        Cue::Over => 0.9,
    }
}

pub fn gap(cue: Cue) -> Duration {
    match cue {
        Cue::Board | Cue::Deliver => BLIP_GAP,
        Cue::Station | Cue::Overflow | Cue::Reject | Cue::Over => Duration::ZERO,
    }
}

pub fn mute_key(ev: &WindowEvent, held: bool) -> bool {
    !held
        && matches!(
            ev,
            WindowEvent::KeyboardInput(KeyboardInput {
                key_code: KeyCode::KeyM,
                state: ButtonState::Pressed,
                repeat: false,
                ..
            })
        )
}

pub fn mute_line(muted: bool) -> String {
    format!("salmong: game mute={}", u8::from(muted))
}

fn track(ev: &WindowEvent, down: &mut [bool; 6]) {
    match ev {
        WindowEvent::KeyboardInput(KeyboardInput {
            key_code, state, ..
        }) => {
            if let Some(k) = MODIFIERS.iter().position(|m| m == key_code) {
                down[k] = *state == ButtonState::Pressed;
            }
        }
        WindowEvent::WindowFocused(f) if !f.focused => *down = [false; 6],
        _ => {}
    }
}

fn load_mute(store: Res<MuteStore>, mut muted: ResMut<Muted>) {
    muted.0 = store.load().as_deref() == Some("1");
}

pub(crate) fn read_mute(
    mut events: MessageReader<WindowEvent>,
    store: Res<MuteStore>,
    mut muted: ResMut<Muted>,
    mut log: ResMut<GameLog>,
    mut down: Local<[bool; 6]>,
    mut warned: Local<bool>,
) {
    for ev in events.read() {
        track(ev, &mut down);
        if !mute_key(ev, down.contains(&true)) {
            continue;
        }
        muted.0 = !muted.0;
        if !store.save(muted.0) && !*warned {
            *warned = true;
            warn!("mute not saved");
        }
        emit(&mut log, mute_line(muted.0));
    }
}

fn pick_cues(
    mut events: MessageReader<SimEventMessage>,
    muted: Res<Muted>,
    real: Res<Time<Real>>,
    mut last: Local<[Option<Duration>; CUES]>,
    mut out: MessageWriter<CueMessage>,
) {
    let now = real.elapsed();
    let mut picked = [false; CUES];
    for ev in events.read() {
        let Some(cue) = cue(&ev.0) else {
            continue;
        };
        let k = cue as usize;
        let early = last[k].is_some_and(|at| now.saturating_sub(at) < gap(cue));
        if muted.0 || picked[k] || early {
            continue;
        }
        picked[k] = true;
        last[k] = Some(now);
        out.write(CueMessage(cue));
    }
}

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MuteStore>()
            .init_resource::<Muted>()
            .init_resource::<GameLog>()
            .add_message::<CueMessage>()
            .add_systems(Startup, load_mute)
            .add_systems(
                Update,
                (
                    read_mute
                        .run_if(not(resource_exists::<ReplayFeed>))
                        .run_if(not(in_state(Screen::Loading))),
                    pick_cues.run_if(not(resource_exists::<ReplayFeed>)),
                )
                    .chain(),
            );
        #[cfg(target_arch = "wasm32")]
        web::build(app);
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::{CUES, Cue, CueMessage, Muted, device, pick_cues, volume, wav};
    use bevy::audio::Volume;
    use bevy::prelude::*;

    const VOICES: usize = 12;
    const ALL: [Cue; CUES] = [
        Cue::Board,
        Cue::Deliver,
        Cue::Station,
        Cue::Overflow,
        Cue::Reject,
        Cue::Over,
    ];

    #[derive(Resource)]
    struct Sounds([Handle<AudioSource>; CUES]);

    #[derive(Component)]
    struct Voice;

    fn load_sounds(mut commands: Commands, mut sources: ResMut<Assets<AudioSource>>) {
        commands.insert_resource(Sounds(ALL.map(|cue| {
            sources.add(AudioSource {
                bytes: wav(cue).into(),
            })
        })));
    }

    fn play_cues(
        mut cues: MessageReader<CueMessage>,
        sounds: Res<Sounds>,
        voices: Query<(), With<Voice>>,
        mut commands: Commands,
        mut warned: Local<bool>,
    ) {
        if cues.is_empty() {
            return;
        }
        if !device::running() {
            cues.clear();
            if !*warned {
                *warned = true;
                warn!("salmong: audio suspended, cue dropped");
            }
            return;
        }
        let mut live = voices.iter().count();
        for &CueMessage(cue) in cues.read() {
            if live >= VOICES && matches!(cue, Cue::Board | Cue::Deliver) {
                continue;
            }
            live += 1;
            commands.spawn((
                AudioPlayer(sounds.0[cue as usize].clone()),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume(cue))),
                Voice,
            ));
        }
    }

    fn hush(muted: Res<Muted>, voices: Query<Entity, With<Voice>>, mut commands: Commands) {
        if !muted.0 {
            return;
        }
        for voice in &voices {
            commands.entity(voice).despawn();
        }
    }

    pub(super) fn build(app: &mut App) {
        app.add_systems(Startup, load_sounds).add_systems(
            Update,
            (
                play_cues.after(pick_cues),
                hush.after(play_cues).run_if(resource_changed::<Muted>),
            ),
        );
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod device {
    use bevy::prelude::Resource;
    use std::env;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    #[derive(Resource)]
    pub struct MuteStore {
        path: Option<PathBuf>,
    }

    impl MuteStore {
        pub fn at(path: PathBuf) -> Self {
            Self { path: Some(path) }
        }

        pub fn load(&self) -> Option<String> {
            fs::read_to_string(self.path.as_ref()?).ok()
        }

        pub fn save(&self, muted: bool) -> bool {
            self.path.as_deref().is_some_and(|path| {
                if muted {
                    write(path).is_ok()
                } else {
                    remove(path).is_ok()
                }
            })
        }
    }

    impl Default for MuteStore {
        fn default() -> Self {
            let data = env::var_os("XDG_DATA_HOME")
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")));
            Self {
                path: data.map(|dir| dir.join("salmong/mute")),
            }
        }
    }

    fn write(path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(path, "1")
    }

    fn remove(path: &Path) -> io::Result<()> {
        match fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod device {
    use bevy::prelude::Resource;
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::Storage;

    const KEY: &str = "salmong.mute";
    const PROBE: &str = "salmongAudio";

    #[derive(Resource, Default)]
    pub struct MuteStore;

    fn storage() -> Option<Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    impl MuteStore {
        pub fn load(&self) -> Option<String> {
            storage()?.get_item(KEY).ok()?
        }

        pub fn save(&self, muted: bool) -> bool {
            storage().is_some_and(|s| {
                if muted {
                    s.set_item(KEY, "1").is_ok()
                } else {
                    s.remove_item(KEY).is_ok()
                }
            })
        }
    }

    fn state() -> Option<String> {
        let window = web_sys::window()?;
        js_sys::Reflect::get(&window, &JsValue::from_str(PROBE))
            .ok()?
            .dyn_into::<js_sys::Function>()
            .ok()?
            .call0(&window)
            .ok()?
            .as_string()
    }

    pub fn running() -> bool {
        state().as_deref() == Some("running")
    }
}
