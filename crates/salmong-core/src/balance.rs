#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Balance {
    pub travel_ticks: u32,
    pub start_buses: u16,
}

impl Balance {
    pub const fn mvp() -> Self {
        Self {
            travel_ticks: 10,
            start_buses: 4,
        }
    }
}
