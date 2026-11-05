pub mod bots;
pub mod generate;
pub mod h32;
pub mod router;

pub use generate::{GEN, GenerateError, generate_pack, generate_scenario, pack_bytes};
pub use h32::{h32, pick, r};
