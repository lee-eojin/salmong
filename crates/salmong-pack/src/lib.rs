use salmong_core::scenario::{RampPoint, Scenario, ScenarioError, Shape, StationSpec};
use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pack {
    pub generation: u32,
    pub pack_seed: u32,
    pub scenarios: Vec<Scenario>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackError {
    Json { message: String },
    Schema { found: String },
    Version { found: u32 },
    UnknownShape { scenario: usize, station: usize, found: u8 },
    NoScenarios,
    ScenarioId { index: usize, found: u32 },
    Scenario { index: usize, error: ScenarioError },
    NonCanonical { offset: usize },
}

pub fn parse_pack(bytes: &[u8]) -> Result<Pack, PackError> {
    let wire: WirePack = serde_json::from_slice(bytes).map_err(|error| PackError::Json {
        message: error.to_string(),
    })?;
    if wire.schema != "salmong.scenario-pack" {
        return Err(PackError::Schema { found: wire.schema });
    }
    if wire.version != 1 {
        return Err(PackError::Version { found: wire.version });
    }
    let mut scenarios = Vec::new();
    for (scenario, row) in wire.scenarios.into_iter().enumerate() {
        let mut stations = Vec::new();
        for (station, (x, y, index, at_tick, weight)) in row.stations.into_iter().enumerate() {
            let shape = Shape::from_index(index).ok_or(PackError::UnknownShape {
                scenario,
                station,
                found: index,
            })?;
            stations.push(StationSpec { x, y, shape, at_tick, weight });
        }
        scenarios.push(Scenario {
            id: row.id,
            seed: row.seed,
            w: row.w,
            h: row.h,
            ramp: row.ramp.into_iter().map(|(tick, rate)| RampPoint { tick, rate }).collect(),
            stations,
        });
    }
    let pack = Pack {
        generation: wire.generation,
        pack_seed: wire.pack_seed,
        scenarios,
    };
    if pack.scenarios.is_empty() {
        return Err(PackError::NoScenarios);
    }
    for (index, scenario) in pack.scenarios.iter().enumerate() {
        if usize::try_from(scenario.id) != Ok(index) {
            return Err(PackError::ScenarioId { index, found: scenario.id });
        }
    }
    for (index, scenario) in pack.scenarios.iter().enumerate() {
        scenario.validate().map_err(|error| PackError::Scenario { index, error })?;
    }
    let canonical = write_pack(&pack);
    if canonical != bytes {
        let offset = canonical
            .iter()
            .zip(bytes)
            .position(|(a, b)| a != b)
            .unwrap_or(canonical.len().min(bytes.len()));
        return Err(PackError::NonCanonical { offset });
    }
    Ok(pack)
}

pub fn write_pack(pack: &Pack) -> Vec<u8> {
    let scenarios = pack.scenarios.iter().map(|scenario| {
        let ramp = scenario.ramp.iter()
            .map(|point| format!("[{},{}]", point.tick, point.rate))
            .collect::<Vec<_>>()
            .join(",");
        let stations = scenario.stations.iter().map(|station| {
            format!("[{},{},{},{},{}]",
                station.x, station.y, station.shape.index(), station.at_tick, station.weight)
        }).collect::<Vec<_>>().join(",");
        format!("{{\"id\":{},\"seed\":{},\"w\":{},\"h\":{},\"ramp\":[{}],\"stations\":[{}]}}",
            scenario.id, scenario.seed, scenario.w, scenario.h, ramp, stations)
    }).collect::<Vec<_>>().join(",\n");
    format!("{{\"schema\":\"salmong.scenario-pack\",\"version\":1,\"gen\":{},\"pack_seed\":{},\"scenarios\":[\n{}\n]}}\n",
        pack.generation, pack.pack_seed, scenarios).into_bytes()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WirePack {
    schema: String,
    version: u32,
    #[serde(rename = "gen")]
    generation: u32,
    pack_seed: u32,
    scenarios: Vec<WireScenario>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireScenario {
    id: u32,
    seed: u32,
    w: u16,
    h: u16,
    ramp: Vec<(u32, u32)>,
    stations: Vec<(u16, u16, u8, u32, u32)>,
}
