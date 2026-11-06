use bevy::prelude::*;
use salmong_pack::{Pack, parse_pack};

const PACK_JSON: &[u8] = include_bytes!("../../../assets/scenarios/pack.v1.json");

#[derive(Resource)]
pub struct ScenarioPack(pub Pack);

pub struct PackPlugin;

impl Plugin for PackPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, load_pack);
    }
}

fn load_pack(mut commands: Commands) {
    let pack = parse_pack(PACK_JSON).expect("embedded pack must parse");
    info!(
        "salmong: scenario pack loaded, {} scenarios",
        pack.scenarios.len()
    );
    commands.insert_resource(ScenarioPack(pack));
}
