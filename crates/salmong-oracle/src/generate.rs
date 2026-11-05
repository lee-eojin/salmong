use salmong_core::scenario::{RampPoint, Scenario, Shape, StationSpec};
use salmong_pack::{Pack, write_pack};

use crate::h32::{h32, pick, r};

pub const GEN: u32 = 1;
pub const MAP_W: u16 = 64;
pub const MAP_H: u16 = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerateError {
    TooFewStations { id: u32, count: usize },
}

pub fn generate_scenario(pack_seed: u32, id: u32) -> Result<Scenario, GenerateError> {
    let seed = h32(pack_seed, id);
    let ramp_rate = pick(80, 100, r(seed, 5, 0, 0));
    let ramp = vec![
        RampPoint { tick: 0, rate: 40 },
        RampPoint {
            tick: 9000,
            rate: ramp_rate,
        },
    ];
    let mut stations = Vec::new();
    for k in 0u32..64 {
        let mut placed = None;
        for a in 0u32..256 {
            let x = pick(2, u32::from(MAP_W) - 3, r(seed, 1, k, a)) as u16;
            let y = pick(2, u32::from(MAP_H) - 3, r(seed, 2, k, a)) as u16;
            let far_enough = stations.iter().all(|station: &StationSpec| {
                let dx = u64::from(station.x.abs_diff(x));
                let dy = u64::from(station.y.abs_diff(y));
                dx * dx + dy * dy >= 16
            });
            if !far_enough {
                continue;
            }
            let at_tick = if k < 3 { 0 } else { (k - 2) * 600 };
            let shape_index = if k < 3 {
                k as u8
            } else if at_tick < 8400 {
                pick(0, 2, r(seed, 3, k, 0)) as u8
            } else {
                pick(0, 3, r(seed, 3, k, 0)) as u8
            };
            let shape = Shape::from_index(shape_index).expect("generator shape fits 0..=3");
            let weight = pick(3, 5, r(seed, 4, k, 0));
            placed = Some(StationSpec {
                x,
                y,
                shape,
                at_tick,
                weight,
            });
            break;
        }
        match placed {
            Some(station) => stations.push(station),
            None => break,
        }
    }
    if stations.len() < 3 {
        return Err(GenerateError::TooFewStations {
            id,
            count: stations.len(),
        });
    }
    Ok(Scenario {
        id,
        seed,
        w: MAP_W,
        h: MAP_H,
        ramp,
        stations,
    })
}

pub fn generate_pack(pack_seed: u32, count: u32) -> Result<Pack, GenerateError> {
    let mut scenarios = Vec::with_capacity(count as usize);
    for id in 0..count {
        scenarios.push(generate_scenario(pack_seed, id)?);
    }
    Ok(Pack {
        generation: GEN,
        pack_seed,
        scenarios,
    })
}

pub fn pack_bytes(pack_seed: u32, count: u32) -> Result<Vec<u8>, GenerateError> {
    Ok(write_pack(&generate_pack(pack_seed, count)?))
}
