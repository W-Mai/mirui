use mirui::{anim::Tween, types::Fixed};
use mirui_sprite_stack::Camera;

use crate::{
    gameplay::PassageFrame,
    world::{INITIAL_EXTENT, Location},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Viewpoint {
    Northwest,
    North,
    Northeast,
    West,
    Overhead,
    East,
    Southwest,
    South,
    Southeast,
    Follow,
}

impl Viewpoint {
    pub(crate) const COMPASS: [[Self; 3]; 3] = [
        [Self::Northwest, Self::North, Self::Northeast],
        [Self::West, Self::Overhead, Self::East],
        [Self::Southwest, Self::South, Self::Southeast],
    ];

    pub(crate) fn next_compact(self) -> Self {
        match self {
            Self::Northeast => Self::Follow,
            Self::Follow => Self::Overhead,
            Self::Overhead => Self::Northwest,
            Self::Northwest => Self::North,
            Self::North => Self::East,
            Self::East => Self::Southeast,
            Self::Southeast => Self::South,
            Self::South => Self::Southwest,
            Self::Southwest => Self::West,
            Self::West => Self::Northeast,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Northwest => "NW",
            Self::North => "N",
            Self::Northeast => "NE",
            Self::West => "W",
            Self::Overhead => "TOP",
            Self::East => "E",
            Self::Southwest => "SW",
            Self::South => "S",
            Self::Southeast => "SE",
            Self::Follow => "FOLLOW",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Northeast => Self::Follow,
            Self::Follow => Self::Overhead,
            _ => Self::Northeast,
        }
    }

    fn frame(self, ship: Location, extent: i32) -> CameraFrame {
        let middle = Fixed::from_int(extent / 2);
        let center = [middle, Fixed::ONE, middle];
        let fit = Fixed::from_ratio(INITIAL_EXTENT, extent);
        let (yaw, pitch) = match self {
            Self::Northwest => (-45, 41),
            Self::North => (0, 41),
            Self::Northeast => (45, 41),
            Self::West => (-90, 41),
            Self::Overhead => (0, 90),
            Self::East => (90, 41),
            Self::Southwest => (-135, 41),
            Self::South => (180, 41),
            Self::Southeast => (135, 41),
            Self::Follow => (-26, 25),
        };
        let (center, scale) = match self {
            Self::Follow => (
                [
                    ship.point[0],
                    ship.height() + Fixed::from_int(2),
                    ship.point[1],
                ],
                Fixed::from_ratio(3, 2),
            ),
            Self::Overhead => (center, fit * Fixed::from_ratio(9, 10)),
            _ => (center, fit),
        };
        CameraFrame {
            center,
            yaw: Fixed::from_int(yaw),
            pitch: Fixed::from_int(pitch),
            scale,
            extent,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct CameraFrame {
    center: [Fixed; 3],
    pub(crate) yaw: Fixed,
    pub(crate) pitch: Fixed,
    scale: Fixed,
    extent: i32,
}

impl CameraFrame {
    fn interpolate(self, next: Self, progress: Fixed) -> Self {
        let between = |a, b| a + (b - a) * progress;
        let half = Fixed::from_int(180);
        let full = Fixed::from_int(360);
        let yaw = next.yaw - self.yaw;
        let yaw = if yaw > half {
            yaw - full
        } else if yaw < -half {
            yaw + full
        } else {
            yaw
        };
        Self {
            center: core::array::from_fn(|i| between(self.center[i], next.center[i])),
            yaw: self.yaw + yaw * progress,
            pitch: between(self.pitch, next.pitch),
            scale: between(self.scale, next.scale),
            extent: next.extent,
        }
    }

    pub(crate) fn camera(
        self,
        base_scale: Fixed,
        compact: bool,
        passage: Option<PassageFrame>,
    ) -> Camera {
        let mut camera = Camera {
            center: self.center,
            yaw: self.yaw,
            pitch: self.pitch,
            scale: base_scale * self.scale,
            range: Fixed::from_int(self.extent * 10 / 9),
            focal_distance: if compact {
                None
            } else {
                Some(Fixed::from_int(230 * self.extent / INITIAL_EXTENT))
            },
        };
        if let Some(passage) = passage {
            let focus = passage.focus * Fixed::from_ratio(3, 4);
            camera.center[0] += (passage.center[0] - camera.center[0]) * focus;
            camera.center[2] += (passage.center[1] - camera.center[2]) * focus;
            camera.scale = camera.scale * (Fixed::ONE + focus);
        }
        camera
    }
}

pub(crate) struct CameraRig {
    origin: CameraFrame,
    viewpoint: Viewpoint,
    transition: Tween,
}

impl CameraRig {
    pub(crate) fn new(ship: Location, extent: i32) -> Self {
        Self {
            origin: Viewpoint::Northeast.frame(ship, extent),
            viewpoint: Viewpoint::Northeast,
            transition: Tween::ease_to(Fixed::ONE, Fixed::ONE, 0),
        }
    }

    pub(crate) fn viewpoint(&self) -> Viewpoint {
        self.viewpoint
    }

    pub(crate) fn frame(&self, ship: Location, extent: i32) -> CameraFrame {
        let target = self.viewpoint.frame(ship, extent);
        if self.transition.is_finished() {
            target
        } else {
            self.origin.interpolate(target, self.transition.value())
        }
    }

    pub(crate) fn select(&mut self, viewpoint: Viewpoint, ship: Location, extent: i32) {
        if self.viewpoint == viewpoint {
            return;
        }
        self.origin = self.frame(ship, extent);
        let half = Fixed::from_int(180);
        let full = Fixed::from_int(360);
        if self.origin.yaw > half {
            self.origin.yaw -= full;
        } else if self.origin.yaw < -half {
            self.origin.yaw += full;
        }
        self.viewpoint = viewpoint;
        self.transition = Tween::ease_to(Fixed::ZERO, Fixed::ONE, 280);
    }

    pub(crate) fn tick(&mut self, delta: u16) {
        self.transition.tick(delta);
    }
}

pub(crate) fn steer_on_screen(right: i8, up: i8, yaw: Fixed) -> [Fixed; 2] {
    if right == 0 && up == 0 {
        return [Fixed::ZERO; 2];
    }
    let sin = Fixed::sin_deg(yaw);
    let cos = Fixed::cos_deg(yaw);
    let (right, up) = (i32::from(right), i32::from(up));
    [cos * right + sin * up, cos * up - sin * right]
}
