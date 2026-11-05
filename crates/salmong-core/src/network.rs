use crate::ids::{Dir, RouteId, StationId};
use crate::scenario::{MAX_STATIONS, Shape};

pub const MAX_ROUTES: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkError {
    TooManyStations,
    TooManyRoutes,
    RouteTooShort(RouteId),
    UnknownStation(RouteId),
    RepeatedStop(RouteId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkSnapshot {
    rev: u64,
    shapes: Vec<Shape>,
    routes: Vec<Option<Vec<StationId>>>,
}

impl NetworkSnapshot {
    pub fn new(
        rev: u64,
        shapes: Vec<Shape>,
        routes: Vec<Option<Vec<StationId>>>,
    ) -> Result<Self, NetworkError> {
        if shapes.len() > MAX_STATIONS {
            return Err(NetworkError::TooManyStations);
        }
        if routes.len() > MAX_ROUTES {
            return Err(NetworkError::TooManyRoutes);
        }
        for (slot, stops) in routes.iter().enumerate() {
            let Some(stops) = stops else { continue };
            let route = RouteId(slot as u8);
            if stops.len() < 2 {
                return Err(NetworkError::RouteTooShort(route));
            }
            if stops.iter().any(|s| usize::from(s.0) >= shapes.len()) {
                return Err(NetworkError::UnknownStation(route));
            }
            for (i, s) in stops.iter().enumerate() {
                if stops[..i].contains(s) {
                    return Err(NetworkError::RepeatedStop(route));
                }
            }
        }
        Ok(Self {
            rev,
            shapes,
            routes,
        })
    }

    pub fn rev(&self) -> u64 {
        self.rev
    }

    pub fn shapes(&self) -> &[Shape] {
        &self.shapes
    }

    pub fn route_slots(&self) -> usize {
        self.routes.len()
    }

    pub fn stops(&self, route: RouteId) -> Option<&[StationId]> {
        self.routes.get(usize::from(route.0))?.as_deref()
    }

    pub fn stop(&self, route: RouteId, dir: Dir, i: usize) -> Option<StationId> {
        let stops = self.stops(route)?;
        let pos = match dir {
            Dir::Fwd => i,
            Dir::Back => stops.len().checked_sub(i + 1)?,
        };
        stops.get(pos).copied()
    }
}
