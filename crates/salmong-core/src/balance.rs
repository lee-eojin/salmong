#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Balance {
    pub travel_ticks: u32,
    pub start_buses: u16,
    pub capacity: usize,
    pub service_ticks: u32,
    pub spawn_unit: u64,
    pub max_alive: usize,
    pub station_capacity: usize,
    pub overflow_limit: u32,
}

impl Balance {
    pub const fn mvp() -> Self {
        Self {
            travel_ticks: 10,
            start_buses: 4,
            capacity: 6,
            service_ticks: 8,
            spawn_unit: 30_000,
            max_alive: 5_000,
            station_capacity: 8,
            overflow_limit: 1350,
        }
    }
}
