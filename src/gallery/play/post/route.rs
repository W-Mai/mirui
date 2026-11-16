use crate::types::{Fixed, Point};

/// The middle row: the inlet, both switches and station B.
pub(crate) const MAIN_Y: i32 = 157;
/// Rows of stations A, B and C.
pub(crate) const STATION_Y: [i32; 3] = [68, MAIN_Y, 246];
/// Where the inlet rail starts.
pub(crate) const INLET_X: i32 = 32;
/// The two switches on the middle row.
pub(crate) const SWITCH_X: [i32; 2] = [151, 265];
/// Where the rails reach the stations.
pub(crate) const STATION_X: i32 = 400;
/// Where the branches to A and C turn level.
pub(crate) const A_BEND_X: i32 = 195;
pub(crate) const C_BEND_X: i32 = 306;
/// The slanted part of each side branch: from S1 up to (195, 68), and from
/// S2 down to (306, 246).
const A_DIAGONAL: Fixed = Fixed::from_ratio(99_282, 1_000);
const C_DIAGONAL: Fixed = Fixed::from_ratio(97_990, 1_000);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PostRoute {
    Entry,
    ToSecondSwitch,
    StationA,
    StationB,
    StationC,
}

impl PostRoute {
    pub(super) const fn terminal_station(self) -> Option<u8> {
        match self {
            Self::StationA => Some(0),
            Self::StationB => Some(1),
            Self::StationC => Some(2),
            Self::Entry | Self::ToSecondSwitch => None,
        }
    }

    pub(super) fn length(self) -> Fixed {
        match self {
            Self::Entry => Fixed::from_int(SWITCH_X[0] - INLET_X),
            Self::ToSecondSwitch => Fixed::from_int(SWITCH_X[1] - SWITCH_X[0]),
            Self::StationA => A_DIAGONAL + Fixed::from_int(STATION_X - A_BEND_X),
            Self::StationB => Fixed::from_int(STATION_X - SWITCH_X[1]),
            Self::StationC => C_DIAGONAL + Fixed::from_int(STATION_X - C_BEND_X),
        }
    }
}

pub(crate) fn position_on_route(route: PostRoute, distance: Fixed) -> Point {
    let main = Fixed::from_int(MAIN_Y);
    match route {
        PostRoute::Entry => Point::new(Fixed::from_int(INLET_X) + distance, main),
        PostRoute::ToSecondSwitch => Point::new(Fixed::from_int(SWITCH_X[0]) + distance, main),
        PostRoute::StationA => polyline_position(
            Point::new(SWITCH_X[0], MAIN_Y),
            Point::new(A_BEND_X, STATION_Y[0]),
            Point::new(STATION_X, STATION_Y[0]),
            A_DIAGONAL,
            distance,
        ),
        PostRoute::StationB => Point::new(Fixed::from_int(SWITCH_X[1]) + distance, main),
        PostRoute::StationC => polyline_position(
            Point::new(SWITCH_X[1], MAIN_Y),
            Point::new(C_BEND_X, STATION_Y[2]),
            Point::new(STATION_X, STATION_Y[2]),
            C_DIAGONAL,
            distance,
        ),
    }
}

fn polyline_position(
    start: Point,
    corner: Point,
    end: Point,
    first_length: Fixed,
    distance: Fixed,
) -> Point {
    if distance <= first_length {
        let t = distance / first_length;
        return Point {
            x: start.x + (corner.x - start.x) * t,
            y: start.y + (corner.y - start.y) * t,
        };
    }
    let remaining = distance - first_length;
    let second_length = (end.x - corner.x).abs() + (end.y - corner.y).abs();
    let t = (remaining / second_length).min(Fixed::ONE);
    Point {
        x: corner.x + (end.x - corner.x) * t,
        y: corner.y + (end.y - corner.y) * t,
    }
}
