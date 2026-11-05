use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use salmong_oracle::generate::{GenerateError, pack_bytes};
use salmong_oracle::trace::{Corruption, balance, corrupt, sample, tla_module};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("usage: salmong-oracle pack <seed> <count> | trace <seed> [drop|swap|alter]");
        return ExitCode::from(2);
    };
    if command == "trace" {
        return trace(args.collect());
    }
    if command != "pack" {
        eprintln!("usage: salmong-oracle pack <seed> <count> | trace <seed> [drop|swap|alter]");
        return ExitCode::from(2);
    }
    let Some(seed_text) = args.next() else {
        eprintln!("usage: salmong-oracle pack <seed> <count> | trace <seed> [drop|swap|alter]");
        return ExitCode::from(2);
    };
    let Some(count_text) = args.next() else {
        eprintln!("usage: salmong-oracle pack <seed> <count> | trace <seed> [drop|swap|alter]");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("usage: salmong-oracle pack <seed> <count> | trace <seed> [drop|swap|alter]");
        return ExitCode::from(2);
    }
    let Ok(seed) = seed_text.parse::<u32>() else {
        eprintln!("seed must be a u32");
        return ExitCode::from(2);
    };
    let Ok(count) = count_text.parse::<u32>() else {
        eprintln!("count must be a u32");
        return ExitCode::from(2);
    };
    if count == 0 {
        eprintln!("count must be at least 1");
        return ExitCode::from(2);
    }
    match pack_bytes(seed, count) {
        Ok(bytes) => {
            if let Err(error) = io::stdout().write_all(&bytes) {
                eprintln!("{error}");
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(GenerateError::TooFewStations { id, count }) => {
            eprintln!("scenario {id} placed only {count} stations");
            ExitCode::FAILURE
        }
    }
}

fn trace(args: Vec<String>) -> ExitCode {
    let usage = || {
        eprintln!("usage: salmong-oracle trace <seed> [drop|swap|alter]");
        ExitCode::from(2)
    };
    let (seed, kind) = match args.as_slice() {
        [seed] => (seed, None),
        [seed, kind] => match kind.as_str() {
            "drop" => (seed, Some(Corruption::Drop)),
            "swap" => (seed, Some(Corruption::Swap)),
            "alter" => (seed, Some(Corruption::Alter)),
            _ => return usage(),
        },
        _ => return usage(),
    };
    let Ok(seed) = seed.parse::<u64>() else {
        return usage();
    };
    let steps = sample(seed);
    let steps = kind.map_or(steps.clone(), |k| corrupt(&steps, k));
    print!("{}", tla_module(&steps, balance().capacity));
    ExitCode::SUCCESS
}
