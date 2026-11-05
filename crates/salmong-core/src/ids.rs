#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StationId(pub u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RouteId(pub u8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dir {
    Fwd = 0,
    Back = 1,
}

impl Dir {
    pub const ALL: [Dir; 2] = [Dir::Fwd, Dir::Back];
    pub fn index(self) -> usize {
        self as usize
    }
}
