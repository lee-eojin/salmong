use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use salmong_oracle::generate::{GenerateError, pack_bytes};

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("usage: salmong-oracle pack <seed> <count>");
        return ExitCode::from(2);
    };
    if command != "pack" {
        eprintln!("usage: salmong-oracle pack <seed> <count>");
        return ExitCode::from(2);
    }
    let Some(seed_text) = args.next() else {
        eprintln!("usage: salmong-oracle pack <seed> <count>");
        return ExitCode::from(2);
    };
    let Some(count_text) = args.next() else {
        eprintln!("usage: salmong-oracle pack <seed> <count>");
        return ExitCode::from(2);
    };
    if args.next().is_some() {
        eprintln!("usage: salmong-oracle pack <seed> <count>");
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
