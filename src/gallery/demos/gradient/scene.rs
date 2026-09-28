use crate::path;
use crate::prelude::draw::*;
use crate::render::scene::GradientStop;

pub(super) const LOGICAL_WIDTH: i32 = 480;
pub(super) const LOGICAL_HEIGHT: i32 = 320;

pub(super) static LINEAR_PANEL: Path = path!(
    M 76 70
    H 180
    Q 208 70 208 98
    V 222
    Q 208 250 180 250
    H 76
    Q 48 250 48 222
    V 98
    Q 48 70 76 70
    Z
);

pub(super) static RADIAL_DISC: Path = path!(
    M 200 100
    C 200 155.228 155.228 200 100 200
    C 44.772 200 0 155.228 0 100
    C 0 44.772 44.772 0 100 0
    C 155.228 0 200 44.772 200 100
    Z
);

pub(super) static LINEAR_STOPS: [GradientStop; 3] = [
    GradientStop {
        offset: mirx::types::Fixed::ZERO,
        color: mirx::types::Color::rgb(50, 120, 255),
    },
    GradientStop {
        offset: mirx::types::Fixed::from_ratio(55, 100),
        color: mirx::types::Color::rgb(125, 90, 255),
    },
    GradientStop {
        offset: mirx::types::Fixed::ONE,
        color: mirx::types::Color::rgb(255, 70, 90),
    },
];

pub(super) static RADIAL_STOPS: [GradientStop; 3] = [
    GradientStop {
        offset: mirx::types::Fixed::ZERO,
        color: mirx::types::Color::rgb(255, 255, 255),
    },
    GradientStop {
        offset: mirx::types::Fixed::from_ratio(42, 100),
        color: mirx::types::Color::rgb(90, 190, 255),
    },
    GradientStop {
        offset: mirx::types::Fixed::ONE,
        color: mirx::types::Color::rgb(20, 70, 190),
    },
];
