use std::env;
use std::fs;
use std::io::{self, Write};
use std::process::ExitCode;

use salmong_oracle::generate::{GenerateError, pack_bytes};
use salmong_oracle::trace::{Corruption, balance, corrupt, sample, tla_module};
use salmong_pack::parse_pack;

const USAGE: &str =
    "usage: salmong-oracle pack <seed> <count> | trace <seed> [drop|swap|alter] | validate <file>";

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if command == "trace" {
        return trace(args.collect());
    }
    if command == "validate" {
        return validate(args.collect());
    }
    if command != "pack" {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let Some(seed_text) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let Some(count_text) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("{USAGE}");
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

fn validate(args: Vec<String>) -> ExitCode {
    let [path] = args.as_slice() else {
        eprintln!("usage: salmong-oracle validate <file>");
        return ExitCode::from(2);
    };
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("{path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    match parse_pack(&bytes) {
        Ok(pack) => {
            println!("ok: {} scenarios", pack.scenarios.len());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error:?}");
            ExitCode::FAILURE
        }
    }
}
