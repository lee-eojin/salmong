use crate::balance::Balance;
use crate::ids::{RouteId, StationId, Tick};
use crate::scenario::Scenario;
use crate::sim::{Command, Sim};

pub const RULES_VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayHeader {
    pub rules_version: u32,
    pub pack_sha256: [u8; 32],
    pub scenario_id: u32,
    pub seed: u64,
    pub balance: Balance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replay {
    pub header: ReplayHeader,
    pub body: Vec<(Tick, Command)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplayError {
    RulesVersion,
    PackSha256,
    ScenarioId,
    Seed,
    Balance,
    Truncated,
    UnknownCommand,
    TrailingBytes,
}

impl ReplayHeader {
    pub fn encode(self) -> Vec<u8> {
        let mut w = Vec::with_capacity(32 + 11 * 8);
        push_u32(&mut w, self.rules_version);
        w.extend_from_slice(&self.pack_sha256);
        push_u32(&mut w, self.scenario_id);
        push_u64(&mut w, self.seed);
        push_u32(&mut w, self.balance.travel_ticks);
        push_u16(&mut w, self.balance.start_buses);
        push_u64(&mut w, self.balance.capacity as u64);
        push_u32(&mut w, self.balance.service_ticks);
        push_u64(&mut w, self.balance.spawn_unit);
        push_u64(&mut w, self.balance.max_alive as u64);
        push_u64(&mut w, self.balance.station_capacity as u64);
        push_u32(&mut w, self.balance.overflow_limit);
        w
    }

    pub fn decode(bytes: &[u8]) -> Result<(Self, &[u8]), ReplayError> {
        let (rules_version, rest) = take_u32(bytes)?;
        let (pack_sha256, rest) = take_array::<32>(rest)?;
        let (scenario_id, rest) = take_u32(rest)?;
        let (seed, rest) = take_u64(rest)?;
        let (travel_ticks, rest) = take_u32(rest)?;
        let (start_buses, rest) = take_u16(rest)?;
        let (capacity, rest) = take_u64(rest)?;
        let (service_ticks, rest) = take_u32(rest)?;
        let (spawn_unit, rest) = take_u64(rest)?;
        let (max_alive, rest) = take_u64(rest)?;
        let (station_capacity, rest) = take_u64(rest)?;
        let (overflow_limit, rest) = take_u32(rest)?;
        let capacity = usize::try_from(capacity).map_err(|_| ReplayError::Truncated)?;
        let max_alive = usize::try_from(max_alive).map_err(|_| ReplayError::Truncated)?;
        let station_capacity =
            usize::try_from(station_capacity).map_err(|_| ReplayError::Truncated)?;
        Ok((
            Self {
                rules_version,
                pack_sha256,
                scenario_id,
                seed,
                balance: Balance {
                    travel_ticks,
                    start_buses,
                    capacity,
                    service_ticks,
                    spawn_unit,
                    max_alive,
                    station_capacity,
                    overflow_limit,
                },
            },
            rest,
        ))
    }

    pub fn matches(
        &self,
        pack_sha256: &[u8; 32],
        scenario: &Scenario,
        seed: u64,
        balance: Balance,
    ) -> Result<(), ReplayError> {
        if self.rules_version != RULES_VERSION {
            return Err(ReplayError::RulesVersion);
        }
        if &self.pack_sha256 != pack_sha256 {
            return Err(ReplayError::PackSha256);
        }
        if self.scenario_id != scenario.id {
            return Err(ReplayError::ScenarioId);
        }
        if self.seed != seed {
            return Err(ReplayError::Seed);
        }
        if self.balance != balance {
            return Err(ReplayError::Balance);
        }
        Ok(())
    }
}

impl Replay {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = self.header.encode();
        push_u64(&mut w, self.body.len() as u64);
        for (tick, cmd) in &self.body {
            push_u64(&mut w, tick.0);
            encode_command(&mut w, cmd);
        }
        w
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ReplayError> {
        let (header, rest) = ReplayHeader::decode(bytes)?;
        let (count, mut rest) = take_u64(rest)?;
        let mut body = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let (tick, next) = take_u64(rest)?;
            let (cmd, next) = decode_command(next)?;
            body.push((Tick(tick), cmd));
            rest = next;
        }
        if !rest.is_empty() {
            return Err(ReplayError::TrailingBytes);
        }
        Ok(Self { header, body })
    }
}

pub fn hash_sequence(
    scenario: Scenario,
    seed: u64,
    balance: Balance,
    body: &[(Tick, Command)],
    ticks: u64,
) -> Vec<u64> {
    let mut sim = Sim::new(scenario, seed, balance);
    let mut hashes = Vec::with_capacity(ticks as usize + 1);
    hashes.push(sim.state_hash());
    let mut events = Vec::new();
    let mut bi = 0usize;
    for t in 0..ticks {
        while bi < body.len() && body[bi].0.0 == t {
            sim.queue(body[bi].1.clone());
            bi += 1;
        }
        events.clear();
        sim.step(&mut events);
        hashes.push(sim.state_hash());
    }
    hashes
}

pub fn run(
    scenario: Scenario,
    pack_sha256: &[u8; 32],
    seed: u64,
    balance: Balance,
    replay: &Replay,
    ticks: u64,
) -> Result<Vec<u64>, ReplayError> {
    replay
        .header
        .matches(pack_sha256, &scenario, seed, balance)?;
    Ok(hash_sequence(scenario, seed, balance, &replay.body, ticks))
}

fn encode_command(w: &mut Vec<u8>, cmd: &Command) {
    match cmd {
        Command::AddRoute { stops } => {
            push_u64(w, 1);
            push_u64(w, 0);
            push_u64(w, stops.len() as u64);
            for s in stops {
                push_u64(w, u64::from(s.0));
            }
        }
        Command::EditRoute { route, stops } => {
            push_u64(w, 2);
            push_u64(w, u64::from(route.0));
            push_u64(w, stops.len() as u64);
            for s in stops {
                push_u64(w, u64::from(s.0));
            }
        }
        Command::RemoveRoute { route } => {
            push_u64(w, 3);
            push_u64(w, u64::from(route.0));
            push_u64(w, 0);
        }
    }
}

fn decode_command(bytes: &[u8]) -> Result<(Command, &[u8]), ReplayError> {
    let (tag, rest) = take_u64(bytes)?;
    let (route, rest) = take_u64(rest)?;
    let (n, mut rest) = take_u64(rest)?;
    let mut stops = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let (s, next) = take_u64(rest)?;
        let s = u16::try_from(s).map_err(|_| ReplayError::Truncated)?;
        stops.push(StationId(s));
        rest = next;
    }
    let route = u8::try_from(route).map_err(|_| ReplayError::Truncated)?;
    let cmd = match tag {
        1 => Command::AddRoute { stops },
        2 => Command::EditRoute {
            route: RouteId(route),
            stops,
        },
        3 => Command::RemoveRoute {
            route: RouteId(route),
        },
        _ => return Err(ReplayError::UnknownCommand),
    };
    Ok((cmd, rest))
}

fn push_u16(w: &mut Vec<u8>, v: u16) {
    w.extend_from_slice(&v.to_le_bytes());
}

fn push_u32(w: &mut Vec<u8>, v: u32) {
    w.extend_from_slice(&v.to_le_bytes());
}

fn push_u64(w: &mut Vec<u8>, v: u64) {
    w.extend_from_slice(&v.to_le_bytes());
}

fn take_array<const N: usize>(bytes: &[u8]) -> Result<([u8; N], &[u8]), ReplayError> {
    if bytes.len() < N {
        return Err(ReplayError::Truncated);
    }
    let mut out = [0u8; N];
    out.copy_from_slice(&bytes[..N]);
    Ok((out, &bytes[N..]))
}

fn take_u16(bytes: &[u8]) -> Result<(u16, &[u8]), ReplayError> {
    let (arr, rest) = take_array::<2>(bytes)?;
    Ok((u16::from_le_bytes(arr), rest))
}

fn take_u32(bytes: &[u8]) -> Result<(u32, &[u8]), ReplayError> {
    let (arr, rest) = take_array::<4>(bytes)?;
    Ok((u32::from_le_bytes(arr), rest))
}

fn take_u64(bytes: &[u8]) -> Result<(u64, &[u8]), ReplayError> {
    let (arr, rest) = take_array::<8>(bytes)?;
    Ok((u64::from_le_bytes(arr), rest))
}

#[cfg(target_arch = "wasm32")]
mod wasm_abi {
    use super::hash_sequence;
    use crate::balance::Balance;
    use crate::scenario::{RampPoint, Scenario, Shape, StationSpec};

    fn empty3_scenario() -> Scenario {
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
                weight: 3,
            })
            .collect(),
        }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn salmong_empty3_hash_count() -> u32 {
        3
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn salmong_empty3_hash_at(index: u32) -> u64 {
        let hashes = hash_sequence(empty3_scenario(), 0, Balance::mvp(), &[], 2);
        hashes.get(index as usize).copied().unwrap_or(0)
    }
}
