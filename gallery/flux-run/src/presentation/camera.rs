use mirui::{anim::Tween, types::Fixed};
use mirui_sprite_stack::Camera;

use crate::{
    gameplay::PassageFrame,
    world::{INITIAL_EXTENT, Location},
};

#[derive(Clone, Copy)]
enum Angle {
    Harbor,
    Follow,
    Survey,
}

impl Angle {
    fn next(self) -> Self {
        match self {
            Self::Harbor => Self::Follow,
            Self::Follow => Self::Survey,
            Self::Survey => Self::Harbor,
        }
    }

    fn frame(self, ship: Location, extent: i32) -> CameraFrame {
        let middle = Fixed::from_int(extent / 2);
        let center = [middle, Fixed::ONE, middle];
        let fit = Fixed::from_ratio(INITIAL_EXTENT, extent);
        match self {
            Self::Harbor => CameraFrame {
                center,
                yaw: Fixed::from_int(45),
                pitch: Fixed::from_int(41),
                scale: fit,
                extent,
            },
            Self::Follow => CameraFrame {
                center: [
                    ship.point[0],
                    ship.height() + Fixed::from_int(2),
                    ship.point[1],
                ],
                yaw: Fixed::from_int(-26),
                pitch: Fixed::from_int(25),
                scale: Fixed::from_ratio(3, 2),
                extent,
            },
            Self::Survey => CameraFrame {
                center,
                yaw: Fixed::ZERO,
                pitch: Fixed::from_int(90),
                scale: fit * Fixed::from_ratio(9, 10),
                extent,
            },
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
        Self {
            center: core::array::from_fn(|i| between(self.center[i], next.center[i])),
            yaw: between(self.yaw, next.yaw),
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
    angle: Angle,
    transition: Tween,
}

impl CameraRig {
    pub(crate) fn new(ship: Location, extent: i32) -> Self {
        Self {
            origin: Angle::Harbor.frame(ship, extent),
            angle: Angle::Harbor,
            transition: Tween::ease_to(Fixed::ONE, Fixed::ONE, 0),
        }
    }

    pub(crate) fn frame(&self, ship: Location, extent: i32) -> CameraFrame {
        let target = self.angle.frame(ship, extent);
        if self.transition.is_finished() {
            target
        } else {
            self.origin.interpolate(target, self.transition.value())
        }
    }

    pub(crate) fn next(&mut self, ship: Location, extent: i32) {
        self.origin = self.frame(ship, extent);
        self.angle = self.angle.next();
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
