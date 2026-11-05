pub const START_STATIONS: usize = 3;
pub const MAX_STATIONS: usize = 64;
pub const MIN_SPACING_SQ: u64 = 16;
pub const STAR_FROM_TICK: u32 = 8400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Circle = 0,
    Triangle = 1,
    Square = 2,
    Star = 3,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Circle, Shape::Triangle, Shape::Square, Shape::Star];
    pub fn from_index(index: u8) -> Option<Shape> {
        Self::ALL.get(usize::from(index)).copied()
    }
    pub fn index(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RampPoint {
    pub tick: u32,
    pub rate: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StationSpec {
    pub x: u16,
    pub y: u16,
    pub shape: Shape,
    pub at_tick: u32,
    pub weight: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    pub id: u32,
    pub seed: u32,
    pub w: u16,
    pub h: u16,
    pub ramp: Vec<RampPoint>,
    pub stations: Vec<StationSpec>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScenarioError {
    EmptyRamp,
    RampNotFromZero { tick: u32 },
    RampNotAscending { index: usize },
    RampFalls { index: usize },
    TooFewStations { count: usize },
    TooManyStations { count: usize },
    SpawnOrder { index: usize },
    StartCount { count: usize },
    StartShapes,
    OutOfBounds { index: usize },
    TooClose { first: usize, second: usize },
    EarlyStar { index: usize },
}

impl Scenario {
    pub fn validate(&self) -> Result<(), ScenarioError> {
        use ScenarioError::*;
        let Some(first) = self.ramp.first() else {
            return Err(EmptyRamp);
        };
        if first.tick != 0 {
            return Err(RampNotFromZero { tick: first.tick });
        }
        for index in 1..self.ramp.len() {
            if self.ramp[index].tick <= self.ramp[index - 1].tick {
                return Err(RampNotAscending { index });
            }
        }
        for index in 1..self.ramp.len() {
            if self.ramp[index].rate < self.ramp[index - 1].rate {
                return Err(RampFalls { index });
            }
        }
        let count = self.stations.len();
        if count < START_STATIONS {
            return Err(TooFewStations { count });
        }
        if count > MAX_STATIONS {
            return Err(TooManyStations { count });
        }
        for index in 1..count {
            if self.stations[index].at_tick < self.stations[index - 1].at_tick {
                return Err(SpawnOrder { index });
            }
        }
        let count = self
            .stations
            .iter()
            .filter(|station| station.at_tick == 0)
            .count();
        if count != START_STATIONS {
            return Err(StartCount { count });
        }
        let mut shapes = self.stations[..START_STATIONS]
            .iter()
            .map(|station| station.shape.index())
            .collect::<Vec<_>>();
        shapes.sort_unstable();
        if shapes != [0, 1, 2] {
            return Err(StartShapes);
        }
        for (index, station) in self.stations.iter().enumerate() {
            if station.x >= self.w || station.y >= self.h {
                return Err(OutOfBounds { index });
            }
        }
        for second in 1..self.stations.len() {
            for first in 0..second {
                let a = self.stations[first];
                let b = self.stations[second];
                let dx = u64::from(a.x.abs_diff(b.x));
                let dy = u64::from(a.y.abs_diff(b.y));
                if dx * dx + dy * dy < MIN_SPACING_SQ {
                    return Err(TooClose { first, second });
                }
            }
        }
        for (index, station) in self.stations.iter().enumerate() {
            if station.shape == Shape::Star && station.at_tick < STAR_FROM_TICK {
                return Err(EarlyStar { index });
            }
        }
        Ok(())
    }
}
