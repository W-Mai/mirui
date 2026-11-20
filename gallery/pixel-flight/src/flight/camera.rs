use super::{Flight, V3, View, follow, smooth, wrap_angle};
use crate::{
    jet,
    math::{FixedMath as _, PI},
    world::{CLOUD_ALT, Ground, MAX_HEIGHT},
};
use mirui::types::{Fixed, Fixed64};

#[derive(Clone, Copy, Debug)]
pub struct Projection {
    pub position: V3,
    pub yaw: Fixed,
    pub camera: mirui_sprite_stack::raster::Projection,
    pub roll: Fixed,
    pub jet_alpha: Fixed,
    pub cockpit: Fixed,
    pub steer_y: Fixed,
}
impl core::ops::Deref for Projection {
    type Target = mirui_sprite_stack::raster::Projection;
    fn deref(&self) -> &Self::Target {
        &self.camera
    }
}
impl core::ops::DerefMut for Projection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.camera
    }
}
impl Projection {
    #[doc = " Returns screen x/y, scale and camera depth. Points behind the eye are rejected."]
    pub fn project(&self, point: V3) -> Option<[Fixed; 4]> {
        self.project_with_yaw(point, self.yaw.sin_cos())
    }
    pub(crate) fn project_with_yaw(
        &self,
        point: V3,
        (sn, cs): (Fixed, Fixed),
    ) -> Option<[Fixed; 4]> {
        let dx = point[0] - self.position[0];
        let dy = point[1] - self.position[1];
        let depth = -(dx * sn + dy * cs);
        self.camera
            .project_camera(dx * cs - dy * sn, point[2] - self.position[2], depth)
    }
    pub fn cockpit_in(&self) -> Fixed {
        let t = ((self.cockpit - Fixed::from_ratio(16, 100)) / Fixed::from_ratio(84, 100))
            .clamp(Fixed::ZERO, Fixed::ONE);
        t * t * t * (t * (t * Fixed::from_int(6) - Fixed::from_int(15)) + Fixed::from_int(10))
    }
}
impl Flight {
    pub fn projection(&self, width: usize, height: usize) -> Projection {
        let s = self.settings;
        let j = &self.jets[0];
        let c = self.chase[2].to_fixed();
        let k = self.cockpit[2].to_fixed();
        let top = Fixed::ONE - c - k;
        let viewport_scale = (Fixed::from_ratio(width as i32, 427))
            .min(Fixed::from_ratio(height as i32, 240))
            .clamp(Fixed::from_ratio(1, 256), Fixed::ONE);
        let mut p = Projection {
            position: self.follow,
            yaw: self.camera_yaw,
            camera: mirui_sprite_stack::raster::Projection {
                perspective: Fixed64::ZERO,
                slope: Fixed::ONE,
                scale: viewport_scale,
                center: [
                    Fixed::from_int((width / 2) as i32),
                    Fixed::from_int((height / 2) as i32),
                ],
            },
            roll: Fixed::ZERO,
            jet_alpha: Fixed::ONE,
            cockpit: k,
            steer_y: Fixed::from_int((height / 2) as i32),
        };
        if c == Fixed::ZERO && k == Fixed::ZERO {
            return p;
        }
        let distance = s.chase_distance.clamp(Fixed::ONE, Fixed::from_int(8))
            * Fixed::from_int((jet::LENGTH) as i32)
            * s.jet_scale
            * (Fixed::ONE - Fixed::from_ratio(16, 100) * self.boost);
        let zoom = (s
            .chase_size
            .clamp(Fixed::from_ratio(5, 100), Fixed::from_ratio(6, 10))
            * Fixed::from_int((width) as i32)
            / (Fixed::from_int((jet::WIDTH) as i32) * s.jet_scale))
            .max(viewport_scale);
        let focal = Fixed::from_int((width) as i32)
            * Fixed::from_ratio(6, 10)
            * (Fixed::ONE - Fixed::from_ratio(1, 10) * self.boost);
        p.position[0] +=
            (j.position[0] + j.yaw.sin() * Fixed::from_int(64) * k - self.follow[0]) * k;
        p.position[1] +=
            (j.position[1] - j.yaw.cos() * Fixed::from_int(64) * k - self.follow[1]) * k;
        p.position[2] += (j.position[2] + Fixed::from_int(3) * s.jet_scale - self.follow[2]) * k;
        p.perspective =
            Fixed64::from_fixed(c) / Fixed64::from_fixed(distance) + Fixed64::from_fixed(k) / 64;
        p.scale =
            (top * viewport_scale.ln() + c * zoom.ln() + k * (focal / Fixed::from_int(64)).ln())
                .exp();
        let lean = ((Fixed::from_int(45) * top
            + s.chase_angle.clamp(Fixed::from_int(2), Fixed::from_int(40)) * c)
            * PI
            / Fixed::from_int(180))
        .tan()
            - Fixed::from_ratio(5, 10) * self.climb_tilt * c;
        p.slope = (lean + p.apply_perspective(self.camera_fix) * (Fixed::ONE - k))
            .max(p.apply_perspective(self.camera_floor - p.position[2]) * (Fixed::ONE - k))
            .max(p.apply_perspective(Fixed::from_int(2) - p.position[2]));
        let pivot = (Fixed::from_int((height) as i32) * Fixed::from_ratio(34, 100)).round();
        let chase_y = Fixed::from_int((height / 2) as i32)
            + Fixed::from_int((height) as i32)
                * (Fixed::from_ratio(14, 100) - Fixed::from_ratio(3, 100) * self.boost);
        let cockpit_y = pivot
            + p.divide_perspective(p.scale)
                * (j.pitch * Fixed::from_ratio(8, 10))
                    .clamp(-Fixed::from_ratio(6, 10), Fixed::from_ratio(6, 10))
                    .tan();
        p.center[1] = top * Fixed::from_int((height / 2) as i32) + c * chase_y + k * cockpit_y;
        p.steer_y = top * Fixed::from_int((height / 2) as i32)
            + c * chase_y
            + k * Fixed::from_int((height) as i32) * Fixed::from_ratio(34, 100);
        p.roll = j.roll * k + j.spin() * self.spin_camera;
        let behind = self.camera_yaw + self.yaw_diff;
        p.yaw = self.camera_yaw + self.yaw_diff * (c + k) + wrap_angle(-j.yaw - behind) * k;
        p.jet_alpha =
            Fixed::ONE - smooth((k - Fixed::from_ratio(12, 100)) / Fixed::from_ratio(4, 10));
        p
    }

    pub(super) fn follow_camera(
        &mut self,
        world: Ground<'_>,
        old_camera: &Projection,
        (sin, cos): (Fixed, Fixed),
        dt: Fixed,
    ) {
        let s = self.settings;
        let targets = [
            Fixed64::from_int(i64::from(self.view == View::Chase)),
            Fixed64::from_int(i64::from(self.view == View::Cockpit)),
        ];
        let ease = Fixed::ONE
            - (-dt * Fixed::from_ratio(76, 10)
                / s.transition
                    .clamp(Fixed::from_ratio(2, 10), Fixed::from_int(8)))
            .exp();
        for (weights, goal) in [
            (&mut self.chase, targets[0]),
            (&mut self.cockpit, targets[1]),
        ] {
            for i in 0..3 {
                weights[i] += ((if i == 0 { goal } else { weights[i - 1] }) - weights[i])
                    * Fixed64::from_fixed(ease);
            }
        }
        if self
            .chase
            .iter()
            .all(|w| (*w - targets[0]).abs() <= Fixed64::from_ratio(1, 512))
            && self
                .cockpit
                .iter()
                .all(|w| (*w - targets[1]).abs() <= Fixed64::from_ratio(1, 512))
        {
            self.chase.fill(targets[0]);
            self.cockpit.fill(targets[1]);
        }
        let player = &self.jets[0];
        let [x, y, altitude] = player.position;
        let look = (Fixed::from_int(22) + Fixed::from_int(16) * self.boost)
            * (Fixed::ONE - self.chase[2].to_fixed() - self.cockpit[2].to_fixed());
        for (k, target) in [x + sin * look, y - cos * look].into_iter().enumerate() {
            self.follow[k] += player.velocity[k] * dt;
            follow(&mut self.follow[k], target, Fixed::from_ratio(35, 10), dt);
        }
        follow(
            &mut self.camera_yaw,
            -player.yaw * s.camera_follow.clamp(Fixed::ZERO, Fixed::ONE),
            Fixed::from_ratio(16, 10),
            dt,
        );
        follow(&mut self.follow[2], altitude, Fixed::from_int(3), dt);
        let mut fix = Fixed::ZERO;
        self.camera_floor = Fixed::ZERO;
        if self.chase[2].to_fixed() > Fixed::from_ratio(3, 10)
            && old_camera.perspective > Fixed64::from_ratio(1, 10000)
        {
            let back = old_camera.divide_perspective(Fixed::ONE);
            let (sn, cs) = old_camera.yaw.sin_cos();
            let x = old_camera.position[0] + sn * back;
            let y = old_camera.position[1] + cs * back;
            let mut high = Fixed::MIN;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    high = high.max(world.height(
                        (x + Fixed::from_int(dx) * Fixed::from_int(9)).trunc_to_int(),
                        (y + Fixed::from_int(dy) * Fixed::from_int(9)).trunc_to_int(),
                    ));
                }
            }
            self.camera_floor = high + Fixed::from_int(5);
            let nominal = self.follow[2] + old_camera.slope * back;
            let mut wanted = nominal.max(self.camera_floor);
            if wanted > CLOUD_ALT - Fixed::from_int(3)
                && wanted < Fixed::from_int((MAX_HEIGHT) as i32) + Fixed::from_int(3)
            {
                wanted = if altitude < CLOUD_ALT - Fixed::from_int(8) {
                    self.camera_floor.max(CLOUD_ALT - Fixed::from_int(3))
                } else {
                    Fixed::from_int((MAX_HEIGHT) as i32) + Fixed::from_int(3)
                };
            }
            fix = wanted - nominal;
        }
        follow(&mut self.camera_fix, fix, Fixed::from_int(7), dt);
        follow(
            &mut self.chase_yaw,
            -player.yaw,
            Fixed::from_ratio(34, 10),
            dt,
        );
        let raw = self.chase_yaw - self.camera_yaw;
        if self.chase[2].to_fixed() == Fixed::ZERO && self.cockpit[2].to_fixed() == Fixed::ZERO {
            self.yaw_diff = wrap_angle(raw);
        } else {
            self.yaw_diff += wrap_angle(raw - self.yaw_diff);
        }
    }
}
