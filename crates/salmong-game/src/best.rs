use bevy::prelude::*;
use salmong_core::score::{FinalScore, merge};

use crate::input::SimRes;

pub use device::BestStore;

#[derive(Resource, Default)]
pub struct BestRes(pub Option<FinalScore>);

pub struct BestPlugin;

impl Plugin for BestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BestStore>()
            .init_resource::<BestRes>()
            .add_systems(Startup, load_best)
            .add_systems(Update, record_best.run_if(resource_exists::<SimRes>));
    }
}

fn load_best(store: Res<BestStore>, mut best: ResMut<BestRes>) {
    best.0 = store.load().as_deref().and_then(FinalScore::decode);
}

pub fn record_best(sim: Res<SimRes>, store: Res<BestStore>, mut best: ResMut<BestRes>) {
    let Some(run) = sim.sim.final_score() else {
        return;
    };
    if best.0 >= Some(run) {
        return;
    }
    let top = merge(store.load().as_deref(), run);
    if !store.save(&top.encode()) {
        warn!("best score not saved");
    }
    best.0 = Some(top);
}

#[cfg(not(target_arch = "wasm32"))]
mod device {
    use bevy::prelude::Resource;
    use std::env;
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    #[derive(Resource)]
    pub struct BestStore {
        path: Option<PathBuf>,
    }

    impl BestStore {
        pub fn at(path: PathBuf) -> Self {
            Self { path: Some(path) }
        }

        pub fn load(&self) -> Option<String> {
            fs::read_to_string(self.path.as_ref()?).ok()
        }

        pub fn save(&self, text: &str) -> bool {
            self.path
                .as_deref()
                .is_some_and(|path| write(path, text).is_ok())
        }
    }

    impl Default for BestStore {
        fn default() -> Self {
            let data = env::var_os("XDG_DATA_HOME")
                .filter(|dir| !dir.is_empty())
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(|home| Path::new(&home).join(".local/share")));
            Self {
                path: data.map(|dir| dir.join("salmong/best")),
            }
        }
    }

    fn write(path: &Path, text: &str) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let scratch = path.with_extension("tmp");
        fs::write(&scratch, text)?;
        fs::rename(scratch, path)
    }
}

#[cfg(target_arch = "wasm32")]
mod device {
    use bevy::prelude::Resource;
    use web_sys::Storage;

    const KEY: &str = "salmong.best";

    #[derive(Resource, Default)]
    pub struct BestStore;

    fn storage() -> Option<Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    impl BestStore {
        pub fn load(&self) -> Option<String> {
            storage()?.get_item(KEY).ok()?
        }

        pub fn save(&self, text: &str) -> bool {
            storage().is_some_and(|s| s.set_item(KEY, text).is_ok())
        }
    }
}
