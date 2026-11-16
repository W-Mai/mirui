//! Flight dynamics and a single camera shared by all three views.

use super::{
    jet,
    world::{CLOUD_ALT, MAX_HEIGHT, World},
};
use crate::{anim::Spring, types::Fixed};
use core::f32::consts::{PI, TAU};

pub const TRAIL_CAPACITY: usize = 160;
pub const SMOKE_CAPACITY: usize = 96;
pub type V3 = [f32; 3];

pub fn wrap_angle(angle: f32) -> f32 {
    angle - TAU * (angle / TAU).round()
}
fn follow(value: &mut f32, target: f32, rate: f32, dt: f32) {
    *value += (target - *value) * (1.0 - (-dt * rate).exp());
}
pub fn smooth(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Overhead,
    Chase,
    Cockpit,
}
impl View {
    pub fn next(self) -> Self {
        match self {
            Self::Overhead => Self::Chase,
            Self::Chase => Self::Cockpit,
            Self::Cockpit => Self::Overhead,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Settings {
    pub speed: f32,
    pub boost: f32,
    pub agility: f32,
    pub bank: f32,
    pub altitude: f32,
    pub ceiling: f32,
    pub clearance: f32,
    pub jet_scale: f32,
    pub spread: f32,
    pub transition: f32,
    pub camera_follow: f32,
    pub chase_distance: f32,
    pub chase_angle: f32,
    pub chase_size: f32,
    pub afterburner: f32,
    pub clouds: f32,
    pub stunts: bool,
    pub intro: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            speed: 92.0,
            boost: 1.8,
            agility: 1.25,
            bank: 52.0,
            altitude: 110.0,
            ceiling: 300.0,
            clearance: 7.0,
            jet_scale: 0.6,
            spread: 1.0,
            transition: 1.4,
            camera_follow: 0.6,
            chase_distance: 2.6,
            chase_angle: 12.0,
            chase_size: 0.19,
            afterburner: 1.0,
            clouds: 0.45,
            stunts: true,
            intro: false,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct TrailPoint {
    pub position: V3,
    pub born: f32,
}
pub struct Aircraft {
    pub position: V3,
    pub yaw: f32,
    pub yaw_rate: f32,
    pub roll: f32,
    pub pitch: f32,
    pub velocity: [f32; 2],
    pub stunt: f32,
    pub stunt_direction: f32,
    pub stunt_delay: f32,
    pub trails: [[TrailPoint; TRAIL_CAPACITY]; 2],
    pub trail_head: usize,
    pub trail_len: usize,
    pub last_trail: [f32; 2],
    pub load: f32,
}
impl Aircraft {
    fn new(position: V3, yaw: f32) -> Self {
        Self {
            position,
            yaw,
            yaw_rate: 0.0,
            roll: 0.0,
            pitch: 0.0,
            velocity: [0.0; 2],
            stunt: -1.0,
            stunt_direction: 1.0,
            stunt_delay: 0.0,
            trails: [[TrailPoint::default(); TRAIL_CAPACITY]; 2],
            trail_head: 0,
            trail_len: 0,
            last_trail: [position[0], position[1]],
            load: 0.0,
        }
    }
    pub fn spin(&self) -> f32 {
        if self.stunt < 0.0 || self.stunt_delay > 0.0 {
            return 0.0;
        }
        let t = self.stunt;
        self.stunt_direction
            * TAU
            * if t < 0.5 {
                2.0 * t * t
            } else {
                1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
            }
    }
    /// Right wing, nose and dorsal axes; a positive bank lowers the right wing.
    pub fn axes(&self) -> [V3; 3] {
        let (sn, cs) = self.yaw.sin_cos();
        let (sr, cr) = (self.roll + self.spin()).sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        [
            [cs * cr, sn * cr, -sr],
            [sn * cp + cs * sr * sp, -cs * cp + sn * sr * sp, cr * sp],
            [cs * sr * cp - sn * sp, sn * sr * cp + cs * sp, cr * cp],
        ]
    }
    pub fn local(&self, local: V3, scale: f32) -> V3 {
        let [right, forward, up] = self.axes();
        core::array::from_fn(|i| {
            self.position[i]
                + scale * (local[0] * right[i] + local[1] * forward[i] + local[2] * up[i])
        })
    }
}

#[derive(Clone, Copy, Default)]
pub struct Smoke {
    pub position: V3,
    pub born: f32,
    pub size: f32,
    pub push: [f32; 2],
    pub strength: f32,
    pub life: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Projection {
    pub position: V3,
    pub yaw: f32,
    pub perspective: f32,
    pub slope: f32,
    pub scale: f32,
    pub center: [f32; 2],
    pub roll: f32,
    pub jet_alpha: f32,
    pub cockpit: f32,
    pub steer_y: f32,
}
impl Projection {
    /// Returns screen x/y, scale and camera depth. Points behind the eye are rejected.
    pub fn project(&self, point: V3) -> Option<[f32; 4]> {
        let (sn, cs) = self.yaw.sin_cos();
        let dx = point[0] - self.position[0];
        let dy = point[1] - self.position[1];
        let depth = -(dx * sn + dy * cs);
        let den = 1.0 + self.perspective * depth;
        if den < 0.12 {
            return None;
        }
        let m = self.scale / den;
        Some([
            self.center[0] + m * (dx * cs - dy * sn),
            self.center[1] - m * (point[2] - self.position[2] + self.slope * depth),
            m,
            depth,
        ])
    }
    pub fn cockpit_in(&self) -> f32 {
        let t = ((self.cockpit - 0.16) / 0.84).clamp(0.0, 1.0);
        t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
    }
}

/// Independent physical key bits keep aliases and simultaneous presses correct.
#[derive(Default)]
pub struct Controls {
    pub keys: u16,
    pub pointer: Option<[f32; 2]>,
    pub pointer_idle: f32,
    pub holding: bool,
    pub hold_time: f32,
    pub keyboard: bool,
}
impl Controls {
    pub fn release_all(&mut self) {
        *self = Self::default();
    }
    pub fn key(&mut self, bit: u8, pressed: bool) {
        if bit >= 10 {
            return;
        }
        if pressed {
            self.keys |= 1 << bit;
        } else {
            self.keys &= !(1 << bit);
        }
        if self.keys & 255 != 0 {
            self.keyboard = true;
        }
    }
    pub fn point(&mut self, position: [f32; 2]) {
        if self.pointer != Some(position) {
            self.pointer_idle = 0.0;
            self.keyboard = false;
        }
        self.pointer = Some(position);
    }
    pub fn press(&mut self) {
        self.holding = true;
        self.hold_time = 0.0;
    }
}

pub struct Flight {
    pub settings: Settings,
    pub jets: [Aircraft; 3],
    pub count: usize,
    pub controls: Controls,
    pub view: View,
    pub time: f32,
    pub intro: f32,
    pub burn: f32,
    pub boost: f32,
    pub speed: f32,
    pub spread: f32,
    pub smoke: [Smoke; SMOKE_CAPACITY],
    pub smoke_head: usize,
    pub chase: [f32; 3],
    pub cockpit: [f32; 3],
    pub head: [Spring; 2],
    follow: V3,
    camera_yaw: f32,
    chase_yaw: f32,
    yaw_diff: f32,
    spin_camera: f32,
    altitude_goal: f32,
    climb_velocity: f32,
    pub climb_tilt: f32,
    floor: f32,
    camera_floor: f32,
    camera_fix: f32,
    auto_stunt: f32,
    smoke_clock: f32,
}
impl Flight {
    pub fn new(world: &World, settings: Settings) -> Self {
        let position = [world.spawn[0], world.spawn[1], settings.altitude];
        Self {
            settings,
            jets: core::array::from_fn(|_| Aircraft::new(position, 0.9)),
            count: 1,
            controls: Controls::default(),
            view: View::Overhead,
            time: 0.0,
            intro: if settings.intro { 0.0 } else { 99.0 },
            burn: if settings.intro { 0.0 } else { 1.0 },
            boost: 0.0,
            speed: settings.speed,
            spread: settings.spread,
            smoke: [Smoke::default(); SMOKE_CAPACITY],
            smoke_head: 0,
            chase: [0.0; 3],
            cockpit: [0.0; 3],
            head: [Spring::with_params(
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(64),
                Fixed::from_int(11),
            ); 2],
            follow: position,
            camera_yaw: -0.9 * settings.camera_follow,
            chase_yaw: -0.9,
            yaw_diff: -0.9 + 0.9 * settings.camera_follow,
            spin_camera: 0.0,
            altitude_goal: settings.altitude,
            climb_velocity: 0.0,
            climb_tilt: 0.0,
            floor: 0.0,
            camera_floor: 0.0,
            camera_fix: 0.0,
            auto_stunt: 7.0,
            smoke_clock: 0.0,
        }
    }
    pub fn set_wingmen(&mut self, count: usize) {
        self.count = 1 + count.min(2);
        for i in 1..self.count {
            let side = if i == 1 { -1.0 } else { 1.0 };
            let p = self.jets[0].local(
                [
                    side * (jet::WIDTH as f32 * 1.2 * self.settings.jet_scale + 2.0),
                    -(jet::LENGTH as f32) * 0.74 * self.settings.jet_scale,
                    0.0,
                ],
                1.0,
            );
            self.jets[i] = Aircraft::new(p, self.jets[0].yaw);
        }
    }
    pub fn cycle_view(&mut self) {
        self.view = self.view.next();
    }
    pub fn roll(&mut self) {
        if self.jets[0].stunt >= 0.0 {
            return;
        }
        self.spin_camera = if self.cockpit[2] > 0.5 { 1.0 } else { 0.0 };
        let direction = if self.jets[0].yaw_rate >= 0.0 {
            1.0
        } else {
            -1.0
        };
        for (i, j) in self.jets[..self.count].iter_mut().enumerate() {
            j.stunt = 0.0;
            j.stunt_direction = direction;
            j.stunt_delay = i as f32 * 0.16;
        }
    }
    pub fn release_pointer(&mut self, cancelled: bool) {
        if !cancelled && self.controls.holding && self.controls.hold_time < 0.2 {
            self.roll();
        }
        self.controls.holding = false;
    }
    pub fn flame_length(&self) -> f32 {
        (0.07 + 0.17 * self.settings.afterburner)
            * jet::LENGTH as f32
            * self.burn
            * (1.6 - self.spread * 0.6).clamp(0.0, 1.0)
            * (1.0 + 1.6 * self.boost)
    }
    pub fn flame_bend(&self) -> f32 {
        (self.jets[0].yaw_rate / self.settings.agility.max(0.1)).clamp(-1.0, 1.0) * 0.36
    }

    pub fn projection(&self, width: usize, height: usize) -> Projection {
        let s = self.settings;
        let j = &self.jets[0];
        let c = self.chase[2];
        let k = self.cockpit[2];
        let top = 1.0 - c - k;
        let mut p = Projection {
            position: self.follow,
            yaw: self.camera_yaw,
            perspective: 0.0,
            slope: 1.0,
            scale: 1.0,
            center: [(width / 2) as f32, (height / 2) as f32],
            roll: 0.0,
            jet_alpha: 1.0,
            cockpit: k,
            steer_y: (height / 2) as f32,
        };
        if c == 0.0 && k == 0.0 {
            return p;
        }
        let distance = s.chase_distance.clamp(1.0, 8.0)
            * jet::LENGTH as f32
            * s.jet_scale
            * (1.0 - 0.16 * self.boost);
        let zoom = (s.chase_size.clamp(0.05, 0.6) * width as f32
            / (jet::WIDTH as f32 * s.jet_scale))
            .max(1.0);
        let focal = width as f32 * 0.6 * (1.0 - 0.1 * self.boost);
        p.position[0] += (j.position[0] + j.yaw.sin() * 64.0 * k - self.follow[0]) * k;
        p.position[1] += (j.position[1] - j.yaw.cos() * 64.0 * k - self.follow[1]) * k;
        p.position[2] += (j.position[2] + 3.0 * s.jet_scale - self.follow[2]) * k;
        p.perspective = c / distance + k / 64.0;
        p.scale = (c * zoom.ln() + k * (focal / 64.0).ln()).exp();
        let lean = ((45.0 * top + s.chase_angle.clamp(2.0, 40.0) * c) * PI / 180.0).tan()
            - 0.5 * self.climb_tilt * c;
        p.slope = (lean + self.camera_fix * p.perspective * (1.0 - k))
            .max((self.camera_floor - p.position[2]) * p.perspective * (1.0 - k))
            .max((2.0 - p.position[2]) * p.perspective);
        let pivot = (height as f32 * 0.34).round();
        let chase_y = (height / 2) as f32 + height as f32 * (0.14 - 0.03 * self.boost);
        let cockpit_y = pivot + p.scale / p.perspective * (j.pitch * 0.8).clamp(-0.6, 0.6).tan();
        p.center[1] = top * (height / 2) as f32 + c * chase_y + k * cockpit_y;
        p.steer_y = top * (height / 2) as f32 + c * chase_y + k * height as f32 * 0.34;
        p.roll = j.roll * k + j.spin() * self.spin_camera;
        let behind = self.camera_yaw + self.yaw_diff;
        p.yaw = self.camera_yaw + self.yaw_diff * (c + k) + wrap_angle(-j.yaw - behind) * k;
        p.jet_alpha = 1.0 - smooth((k - 0.12) / 0.4);
        p
    }

    pub fn step(&mut self, world: &World, seconds: f32, width: usize, height: usize) {
        let dt = if seconds.is_finite() {
            seconds.clamp(0.0, 0.05)
        } else {
            0.0
        };
        if dt == 0.0 {
            return;
        }
        let s = self.settings;
        let old_camera = self.projection(width, height);
        self.time += dt;
        self.intro += dt;
        self.controls.pointer_idle += dt;
        if self.controls.holding {
            self.controls.hold_time += dt;
        }
        let ready = self.intro > 1.9;
        follow(&mut self.burn, if ready { 1.0 } else { 0.0 }, 5.0, dt);
        let pushing = ready
            && ((self.controls.holding && self.controls.hold_time >= 0.2)
                || self.controls.keys & 768 != 0);
        follow(
            &mut self.boost,
            if pushing { 1.0 } else { 0.0 },
            if pushing { 5.0 } else { 2.4 },
            dt,
        );
        if !pushing && self.boost < 0.0005 {
            self.boost = 0.0;
        }
        self.speed = s.speed * (1.0 + (s.boost.clamp(1.0, 4.0) - 1.0) * self.boost);
        follow(&mut self.spread, s.spread, 11.0, dt);
        let j = &self.jets[0];
        let rate = s.agility.max(0.1);
        let clearance = s.clearance.clamp(2.0, 60.0);
        let mut ground: u8 = 0;
        for i in 0..=4 {
            for side in -1..=1 {
                let distance = self.speed * 0.16 * i as f32;
                ground = ground.max(world.height(
                    (j.position[0] + j.yaw.sin() * distance + j.yaw.cos() * side as f32 * 7.0)
                        as i32,
                    (j.position[1] - j.yaw.cos() * distance + j.yaw.sin() * side as f32 * 7.0)
                        as i32,
                ));
            }
        }
        let floor = f32::from(ground) + clearance;
        if self.floor == 0.0 {
            self.floor = floor;
        }
        let floor_rate = if floor > self.floor { 9.0 } else { 3.0 };
        follow(&mut self.floor, floor, floor_rate, dt);
        let ceiling = self.floor.max(s.altitude).max(s.ceiling.clamp(20.0, 900.0));
        let climb_max = self.speed * 0.55;
        let chase = self.chase[2] + self.cockpit[2] > 0.5;
        let mut flown = false;
        let mut want = 0.0;
        if ready {
            if self.controls.keyboard {
                let bits = self.controls.keys;
                want = (i32::from(bits & 12 != 0) - i32::from(bits & 3 != 0)) as f32 * rate;
                self.altitude_goal += (i32::from(bits & 48 != 0) - i32::from(bits & 192 != 0))
                    as f32
                    * climb_max
                    * dt;
                flown = true;
            } else if let Some(pointer) = self
                .controls
                .pointer
                .filter(|_| self.controls.pointer_idle < 5.0)
            {
                if chase {
                    let turn = ((pointer[0] - old_camera.center[0]) / (width as f32 * 0.26))
                        .clamp(-1.0, 1.0);
                    if turn.abs() > 0.06 {
                        want = turn * rate;
                    }
                    let y = old_camera.steer_y;
                    let raw = if pointer[1] < y {
                        (y - pointer[1]) / y.max(1.0)
                    } else {
                        -(pointer[1] - y) / (height as f32 - y).max(1.0)
                    };
                    let up = raw.signum() * (raw.abs() - 0.08).max(0.0) / 0.92;
                    self.altitude_goal = if up >= 0.0 {
                        s.altitude + up * (ceiling - s.altitude)
                    } else {
                        s.altitude + up * (s.altitude - self.floor)
                    };
                    flown = true;
                } else if let Some(point) = old_camera.project(j.position) {
                    let dx = pointer[0] - point[0];
                    let dy = pointer[1] - point[1];
                    if dx * dx + dy * dy > 324.0 {
                        let (sn, cs) = old_camera.yaw.sin_cos();
                        want =
                            (wrap_angle((dx * cs + dy * sn).atan2(-(-dx * sn + dy * cs)) - j.yaw)
                                * 2.6)
                                .clamp(-rate, rate);
                    }
                    flown = true;
                }
            } else {
                want = (rate
                    * (0.62 * (self.time * 0.31 + 1.3).sin()
                        + 0.38 * (self.time * 0.73 + 0.4).sin()
                        + 0.22 * (self.time * 0.17).sin()))
                .clamp(-rate, rate);
                if s.stunts {
                    self.auto_stunt -= dt;
                    if self.auto_stunt <= 0.0 {
                        self.roll();
                        self.auto_stunt = 9.0
                            + f32::from(super::world::hash8((self.time * 10.0) as u32)) / 255.0
                                * 7.0;
                    }
                }
            }
        }
        let j = &mut self.jets[0];
        follow(&mut j.yaw_rate, want, 2.6, dt);
        j.yaw += j.yaw_rate * dt;
        follow(
            &mut j.roll,
            (j.yaw_rate / rate).clamp(-1.0, 1.0) * s.bank * PI / 180.0,
            5.0,
            dt,
        );
        j.velocity = [j.yaw.sin() * self.speed, -j.yaw.cos() * self.speed];
        j.position[0] += j.velocity[0] * dt;
        j.position[1] += j.velocity[1] * dt;
        if !flown {
            follow(&mut self.altitude_goal, s.altitude, 0.9, dt);
        }
        self.altitude_goal = self.altitude_goal.clamp(self.floor, ceiling);
        let dz = ((self.altitude_goal + (self.time * 0.9).sin() * 1.5 - j.position[2])
            * (1.0 - (-dt * 3.0).exp()))
        .clamp(-climb_max * dt, climb_max * dt);
        j.position[2] += dz;
        let hard =
            f32::from(world.height(j.position[0] as i32, j.position[1] as i32)) + clearance * 0.7;
        if j.position[2] < hard {
            follow(&mut j.position[2], hard, 22.0, dt);
        }
        follow(&mut self.climb_velocity, dz / dt.max(1e-4), 6.0, dt);
        self.climb_tilt = (self.climb_velocity.signum()
            * (self.climb_velocity.abs() - 3.0).max(0.0)
            / self.speed.max(20.0)
            * 1.25)
            .clamp(-0.5, 0.5);
        follow(
            &mut j.pitch,
            (j.yaw_rate / rate).abs() * 0.1 + self.climb_tilt,
            4.0,
            dt,
        );
        let goals = [
            (j.yaw_rate / rate).clamp(-1.0, 1.0) * 6.4 + (self.time * 0.53).sin() * 0.3,
            (self.climb_tilt / 0.5).clamp(-1.0, 1.0) * 3.4
                + self.boost * 1.6
                + (self.time * 0.71 + 1.0).sin() * 0.25,
        ];
        for (spring, goal) in self.head.iter_mut().zip(goals) {
            spring.retarget(Fixed::from_f32(goal), None);
            spring.tick((dt * 1000.0).round() as u16);
        }
        let position = j.position;
        let yaw = j.yaw;
        let velocity = j.velocity;
        let roll = j.roll;
        let pitch = j.pitch;
        let yaw_rate = j.yaw_rate;
        for i in 1..self.count {
            let j = &mut self.jets[i];
            let side = if i == 1 { -1.0 } else { 1.0 };
            let x = side * (jet::WIDTH as f32 * 1.2 * s.jet_scale + 2.0);
            let y = jet::LENGTH as f32 * 0.74 * s.jet_scale;
            let target = [
                position[0] + yaw.cos() * x - yaw.sin() * y,
                position[1] + yaw.sin() * x + yaw.cos() * y,
            ];
            for k in 0..2 {
                let old = j.position[k];
                j.position[k] += velocity[k] * dt;
                follow(&mut j.position[k], target[k], 3.2, dt);
                j.velocity[k] = (j.position[k] - old) / dt;
            }
            j.yaw += wrap_angle(yaw - j.yaw) * (1.0 - (-dt * 4.0).exp());
            follow(&mut j.roll, roll, 3.2, dt);
            j.pitch = pitch;
            follow(
                &mut j.position[2],
                position[2] + (self.time * 1.1 + i as f32 * 2.0).sin() * 1.2,
                3.0,
                dt,
            );
            j.position[2] = j.position[2].max(
                f32::from(world.height(j.position[0] as i32, j.position[1] as i32))
                    + clearance * 0.7,
            );
        }
        for j in &mut self.jets[..self.count] {
            let load = (yaw_rate.abs() / rate
                + if j.stunt >= 0.0 { 0.7 } else { 0.0 }
                + self.boost * 0.9
                + self.climb_tilt.abs())
            .clamp(0.0, 1.0);
            follow(&mut j.load, load, 2.5, dt);
            if j.stunt >= 0.0 {
                if j.stunt_delay > 0.0 {
                    j.stunt_delay -= dt;
                } else {
                    j.stunt += dt / 0.85;
                    if j.stunt >= 1.0 {
                        j.stunt = -1.0;
                    }
                }
            }
            if (j.position[0] - j.last_trail[0]).powi(2) + (j.position[1] - j.last_trail[1]).powi(2)
                > 6.25
            {
                j.last_trail = [j.position[0], j.position[1]];
                for w in 0..2 {
                    j.trails[w][j.trail_head] = TrailPoint {
                        position: j.local(
                            [
                                if w == 0 { -15.5 } else { 15.5 },
                                jet::PIVOT[0] - 33.0,
                                4.0 - jet::PIVOT[1],
                            ],
                            s.jet_scale,
                        ),
                        born: self.time,
                    };
                }
                j.trail_head = (j.trail_head + 1) % TRAIL_CAPACITY;
                j.trail_len = (j.trail_len + 1).min(TRAIL_CAPACITY);
            }
        }
        self.smoke_clock += dt;
        if self.smoke_clock >= 0.034 - 0.012 * self.boost
            && self.burn > 0.6
            && s.afterburner > 0.05
            && self.spread < 2.0
        {
            self.smoke_clock = 0.0;
            let length = self.flame_length();
            let bend = self.flame_bend();
            let t = length * 0.78;
            for (i, j) in self.jets[..self.count].iter().enumerate() {
                let head = self.smoke_head;
                let side = if (head + i) & 1 != 0 { 1.0 } else { -1.0 };
                self.smoke[head] = Smoke {
                    position: j.local(
                        [
                            side * 1.96 + bend * t * t / length.max(1.0),
                            jet::PIVOT[0] - 43.6 - t,
                            4.1 - jet::PIVOT[1],
                        ],
                        s.jet_scale,
                    ),
                    born: self.time,
                    size: (1.3
                        + f32::from(super::world::hash8((head * 31 + 7) as u32)) / 255.0 * 1.2)
                        * s.jet_scale,
                    push: [
                        -velocity[0] * 0.3
                            + (f32::from(super::world::hash8((head * 17 + 3) as u32)) - 128.0)
                                * 0.05,
                        -velocity[1] * 0.3
                            + (f32::from(super::world::hash8((head * 13 + 5) as u32)) - 128.0)
                                * 0.05,
                    ],
                    strength: (0.46 + 0.36 * self.boost) * (s.afterburner * 1.4).clamp(0.0, 1.0),
                    life: 0.75
                        + 0.5 * self.boost
                        + f32::from(super::world::hash8((head * 5) as u32)) / 255.0 * 0.3,
                };
                self.smoke_head = (head + 1) % SMOKE_CAPACITY;
            }
        }
        // Cascaded camera filters preserve the published pose blend and retarget continuously.
        let targets = [
            f32::from(self.view == View::Chase),
            f32::from(self.view == View::Cockpit),
        ];
        let ease = 1.0 - (-dt * 7.6 / s.transition.clamp(0.2, 8.0)).exp();
        for (weights, goal) in [
            (&mut self.chase, targets[0]),
            (&mut self.cockpit, targets[1]),
        ] {
            for i in 0..3 {
                weights[i] += ((if i == 0 { goal } else { weights[i - 1] }) - weights[i]) * ease;
            }
        }
        if self.chase.iter().all(|w| (*w - targets[0]).abs() < 0.0004)
            && self
                .cockpit
                .iter()
                .all(|w| (*w - targets[1]).abs() < 0.0004)
        {
            self.chase.fill(targets[0]);
            self.cockpit.fill(targets[1]);
        }
        let look = (22.0 + 16.0 * self.boost) * (1.0 - self.chase[2] - self.cockpit[2]);
        for (k, target) in [
            position[0] + yaw.sin() * look,
            position[1] - yaw.cos() * look,
        ]
        .into_iter()
        .enumerate()
        {
            self.follow[k] += velocity[k] * dt;
            follow(&mut self.follow[k], target, 3.5, dt);
        }
        follow(
            &mut self.camera_yaw,
            -yaw * s.camera_follow.clamp(0.0, 1.0),
            1.6,
            dt,
        );
        follow(&mut self.follow[2], position[2], 3.0, dt);
        let mut fix = 0.0;
        self.camera_floor = 0.0;
        if self.chase[2] > 0.3 && old_camera.perspective > 1e-4 {
            let back = 1.0 / old_camera.perspective;
            let (sn, cs) = old_camera.yaw.sin_cos();
            let x = old_camera.position[0] + sn * back;
            let y = old_camera.position[1] + cs * back;
            let mut high = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    high = high.max(
                        world.height((x + dx as f32 * 9.0) as i32, (y + dy as f32 * 9.0) as i32),
                    );
                }
            }
            self.camera_floor = f32::from(high) + 5.0;
            let nominal = self.follow[2] + old_camera.slope * back;
            let mut wanted = nominal.max(self.camera_floor);
            if wanted > CLOUD_ALT - 3.0 && wanted < MAX_HEIGHT as f32 + 3.0 {
                wanted = if position[2] < CLOUD_ALT - 8.0 {
                    self.camera_floor.max(CLOUD_ALT - 3.0)
                } else {
                    MAX_HEIGHT as f32 + 3.0
                };
            }
            fix = wanted - nominal;
        }
        follow(&mut self.camera_fix, fix, 7.0, dt);
        follow(&mut self.chase_yaw, -yaw, 3.4, dt);
        let raw = self.chase_yaw - self.camera_yaw;
        if self.chase[2] == 0.0 && self.cockpit[2] == 0.0 {
            self.yaw_diff = wrap_angle(raw);
        } else {
            self.yaw_diff += wrap_angle(raw - self.yaw_diff);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_retargets_without_teleporting_and_all_views_settle() {
        let world = World::new(7, 0.33);
        let mut flight = Flight::new(&world, Settings::default());
        for target in [View::Chase, View::Cockpit, View::Overhead, View::Cockpit] {
            let before = flight.projection(427, 240);
            flight.view = target;
            assert_eq!(before.center, flight.projection(427, 240).center);
            for _ in 0..400 {
                flight.step(&world, 0.016, 427, 240);
                let p = flight.projection(427, 240);
                assert!(p.center.into_iter().chain(p.position).all(f32::is_finite));
            }
            assert_eq!(flight.cockpit[2], f32::from(target == View::Cockpit));
            assert_eq!(flight.chase[2], f32::from(target == View::Chase));
        }
        for i in 0..300 {
            flight.view = if i % 2 == 0 {
                View::Chase
            } else {
                View::Cockpit
            };
            flight.step(&world, 0.016, 427, 240);
            assert!(flight.chase[2] + flight.cockpit[2] <= 1.00001);
        }
    }
    #[test]
    fn controls_cancel_and_history_storage_remain_bounded() {
        let world = World::new(7, 0.33);
        let mut flight = Flight::new(&world, Settings::default());
        flight.set_wingmen(2);
        flight.controls.key(0, true);
        flight.controls.key(1, true);
        flight.controls.key(0, false);
        assert_eq!(flight.controls.keys, 2);
        flight.controls.press();
        flight.release_pointer(true);
        assert!(flight.jets[0].stunt < 0.0);
        flight.controls.press();
        flight.release_pointer(false);
        assert_eq!(flight.jets[0].stunt, 0.0);
        flight.controls.release_all();
        assert_eq!(flight.controls.keys, 0);
        let allocations = crate::test_support::tracked_allocations(|| {
            for _ in 0..2000 {
                flight.step(&world, 0.016, 427, 240);
            }
        });
        assert_eq!(allocations, 0);
        for jet in &flight.jets {
            assert!(jet.trail_len <= TRAIL_CAPACITY);
            assert!(jet.trail_head < TRAIL_CAPACITY);
            assert!(jet.position.iter().all(|v| v.is_finite()));
        }
        assert!(flight.smoke_head < SMOKE_CAPACITY);
    }
}
