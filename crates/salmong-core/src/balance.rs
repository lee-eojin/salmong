#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Balance {
    pub travel_ticks: u32,
    pub start_buses: u16,
    pub capacity: usize,
    pub service_ticks: u32,
}

impl Balance {
    pub const fn mvp() -> Self {
        Self {
            travel_ticks: 10,
            start_buses: 4,
            capacity: 6,
            service_ticks: 8,
        }
    }
}
