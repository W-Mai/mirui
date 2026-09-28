use crate::types::{Fixed, Point};

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
            Self::Entry => Fixed::from_int(119),
            Self::ToSecondSwitch => Fixed::from_int(114),
            Self::StationA => Fixed::from_ratio(285_994, 1_000),
            Self::StationB => Fixed::from_int(135),
            Self::StationC => Fixed::from_ratio(173_404, 1_000),
        }
    }
}

pub(crate) fn position_on_route(route: PostRoute, distance: Fixed) -> Point {
    match route {
        PostRoute::Entry => Point::new(Fixed::from_int(32) + distance, Fixed::from_int(157)),
        PostRoute::ToSecondSwitch => {
            Point::new(Fixed::from_int(151) + distance, Fixed::from_int(157))
        }
        PostRoute::StationA => polyline_position(
            Point::new(Fixed::from_int(151), Fixed::from_int(157)),
            Point::new(Fixed::from_int(195), Fixed::from_int(89)),
            Point::new(Fixed::from_int(400), Fixed::from_int(89)),
            Fixed::from_ratio(80_994, 1_000),
            distance,
        ),
        PostRoute::StationB => Point::new(Fixed::from_int(265) + distance, Fixed::from_int(157)),
        PostRoute::StationC => polyline_position(
            Point::new(Fixed::from_int(265), Fixed::from_int(157)),
            Point::new(Fixed::from_int(306), Fixed::from_int(225)),
            Point::new(Fixed::from_int(400), Fixed::from_int(225)),
            Fixed::from_ratio(79_404, 1_000),
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
