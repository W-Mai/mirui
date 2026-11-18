#![doc = " Flight dynamics and a single camera shared by all three views."]
use super::math::FixedMath as _;
use super::math::{PI, TAU};
use super::{
    jet,
    world::{CLOUD_ALT, MAX_HEIGHT, World},
};
use crate::{
    anim::Spring,
    types::{Fixed, Fixed64},
};
use alloc::{boxed::Box, vec::Vec};
pub const TRAIL_CAPACITY: usize = 160;
pub const SMOKE_CAPACITY: usize = 96;
pub type V3 = [Fixed; 3];
pub fn wrap_angle(angle: Fixed) -> Fixed {
    angle - TAU * (angle / TAU).round()
}
fn follow(value: &mut Fixed, target: Fixed, rate: Fixed, dt: Fixed) {
    *value += (target - *value) * (Fixed::ONE - (-dt * rate).exp());
}
pub fn smooth(value: Fixed) -> Fixed {
    let t = value.clamp(Fixed::ZERO, Fixed::ONE);
    t * t * (Fixed::from_int(3) - Fixed::from_int(2) * t)
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
    pub speed: Fixed,
    pub boost: Fixed,
    pub agility: Fixed,
    pub bank: Fixed,
    pub altitude: Fixed,
    pub ceiling: Fixed,
    pub clearance: Fixed,
    pub jet_scale: Fixed,
    pub spread: Fixed,
    pub transition: Fixed,
    pub camera_follow: Fixed,
    pub chase_distance: Fixed,
    pub chase_angle: Fixed,
    pub chase_size: Fixed,
    pub afterburner: Fixed,
    pub clouds: Fixed,
    pub stunts: bool,
    pub intro: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            speed: Fixed::from_int(92),
            boost: Fixed::from_ratio(18, 10),
            agility: Fixed::from_ratio(125, 100),
            bank: Fixed::from_int(52),
            altitude: Fixed::from_int(110),
            ceiling: Fixed::from_int(300),
            clearance: Fixed::from_int(7),
            jet_scale: Fixed::from_ratio(6, 10),
            spread: Fixed::ONE,
            transition: Fixed::from_ratio(14, 10),
            camera_follow: Fixed::from_ratio(6, 10),
            chase_distance: Fixed::from_ratio(26, 10),
            chase_angle: Fixed::from_int(12),
            chase_size: Fixed::from_ratio(19, 100),
            afterburner: Fixed::ONE,
            clouds: Fixed::from_ratio(45, 100),
            stunts: true,
            intro: false,
        }
    }
}
#[derive(Clone, Copy, Default)]
pub struct TrailPoint {
    pub position: V3,
    pub born: Fixed,
}
pub struct Aircraft {
    pub position: V3,
    pub yaw: Fixed,
    pub yaw_rate: Fixed,
    pub roll: Fixed,
    pub pitch: Fixed,
    pub velocity: [Fixed; 2],
    pub stunt: Fixed,
    pub stunt_direction: Fixed,
    pub stunt_delay: Fixed,
    pub trails: [[TrailPoint; TRAIL_CAPACITY]; 2],
    pub trail_head: usize,
    pub trail_len: usize,
    pub last_trail: [Fixed; 2],
    pub load: Fixed,
}
impl Aircraft {
    fn new(position: V3, yaw: Fixed) -> Self {
        Self {
            position,
            yaw,
            yaw_rate: Fixed::ZERO,
            roll: Fixed::ZERO,
            pitch: Fixed::ZERO,
            velocity: [Fixed::ZERO; 2],
            stunt: -Fixed::ONE,
            stunt_direction: Fixed::ONE,
            stunt_delay: Fixed::ZERO,
            trails: [[TrailPoint::default(); TRAIL_CAPACITY]; 2],
            trail_head: 0,
            trail_len: 0,
            last_trail: [position[0], position[1]],
            load: Fixed::ZERO,
        }
    }
    pub fn spin(&self) -> Fixed {
        if self.stunt < Fixed::ZERO || self.stunt_delay > Fixed::ZERO {
            return Fixed::ZERO;
        }
        let t = self.stunt;
        self.stunt_direction
            * TAU
            * if t < Fixed::from_ratio(5, 10) {
                Fixed::from_int(2) * t * t
            } else {
                Fixed::ONE
                    - (-Fixed::from_int(2) * t + Fixed::from_int(2)).powi(2) / Fixed::from_int(2)
            }
    }
    #[doc = " Right wing, nose and dorsal axes; a positive bank lowers the right wing."]
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
    pub fn local(&self, local: V3, scale: Fixed) -> V3 {
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
    pub born: Fixed,
    pub size: Fixed,
    pub push: [Fixed; 2],
    pub strength: Fixed,
    pub life: Fixed,
}
#[derive(Clone, Copy, Debug)]
pub struct Projection {
    pub position: V3,
    pub yaw: Fixed,
    #[doc = " Reciprocal camera distance needs fractions below one Q24.8 unit."]
    pub perspective: Fixed64,
    pub slope: Fixed,
    pub scale: Fixed,
    pub center: [Fixed; 2],
    pub roll: Fixed,
    pub jet_alpha: Fixed,
    pub cockpit: Fixed,
    pub steer_y: Fixed,
}
impl Projection {
    pub fn apply_perspective(&self, value: Fixed) -> Fixed {
        (self.perspective * Fixed64::from_fixed(value)).to_fixed()
    }
    pub fn divide_perspective(&self, value: Fixed) -> Fixed {
        (Fixed64::from_fixed(value) / self.perspective).to_fixed()
    }
    #[doc = " Returns screen x/y, scale and camera depth. Points behind the eye are rejected."]
    pub fn project(&self, point: V3) -> Option<[Fixed; 4]> {
        self.project_with_yaw(point, self.yaw.sin_cos())
    }
    pub(super) fn project_with_yaw(
        &self,
        point: V3,
        (sn, cs): (Fixed, Fixed),
    ) -> Option<[Fixed; 4]> {
        let dx = point[0] - self.position[0];
        let dy = point[1] - self.position[1];
        let depth = -(dx * sn + dy * cs);
        let den = Fixed::ONE + self.apply_perspective(depth);
        if den < Fixed::from_ratio(12, 100) {
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
    pub fn cockpit_in(&self) -> Fixed {
        let t = ((self.cockpit - Fixed::from_ratio(16, 100)) / Fixed::from_ratio(84, 100))
            .clamp(Fixed::ZERO, Fixed::ONE);
        t * t * t * (t * (t * Fixed::from_int(6) - Fixed::from_int(15)) + Fixed::from_int(10))
    }
}
#[doc = " Independent physical key bits keep aliases and simultaneous presses correct."]
#[derive(Default)]
pub struct Controls {
    pub keys: u16,
    pub pointer: Option<[Fixed; 2]>,
    pub pointer_idle: Fixed,
    pub holding: bool,
    pub hold_time: Fixed,
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
    pub fn point(&mut self, position: [Fixed; 2]) {
        if self.pointer != Some(position) {
            self.pointer_idle = Fixed::ZERO;
            self.keyboard = false;
        }
        self.pointer = Some(position);
    }
    pub fn press(&mut self) {
        self.holding = true;
        self.hold_time = Fixed::ZERO;
    }
}
pub struct Flight {
    pub settings: Settings,
    pub jets: Box<[Aircraft; 3]>,
    pub count: usize,
    pub controls: Controls,
    pub view: View,
    pub time: Fixed,
    elapsed_ms: u64,
    pub intro: Fixed,
    pub burn: Fixed,
    pub boost: Fixed,
    pub speed: Fixed,
    pub spread: Fixed,
    pub smoke: [Smoke; SMOKE_CAPACITY],
    pub smoke_head: usize,
    pub chase: [Fixed64; 3],
    pub cockpit: [Fixed64; 3],
    pub head: [Spring; 2],
    follow: V3,
    camera_yaw: Fixed,
    chase_yaw: Fixed,
    yaw_diff: Fixed,
    spin_camera: Fixed,
    altitude_goal: Fixed,
    climb_velocity: Fixed,
    pub climb_tilt: Fixed,
    floor: Fixed,
    camera_floor: Fixed,
    camera_fix: Fixed,
    auto_stunt: Fixed,
    smoke_clock: Fixed,
}
impl Flight {
    pub fn new(world: &World, settings: Settings) -> Self {
        let position = [world.spawn[0], world.spawn[1], settings.altitude];
        // Keep fixed histories out of by-value Model construction temporaries.
        let mut jets = Vec::with_capacity(3);
        for _ in 0..3 {
            jets.push(Aircraft::new(position, Fixed::from_ratio(9, 10)));
        }
        Self {
            settings,
            jets: jets
                .into_boxed_slice()
                .try_into()
                .unwrap_or_else(|_| unreachable!("exactly three aircraft")),
            count: 1,
            controls: Controls::default(),
            view: View::Overhead,
            time: Fixed::ZERO,
            elapsed_ms: 0,
            intro: if settings.intro {
                Fixed::ZERO
            } else {
                Fixed::from_int(99)
            },
            burn: if settings.intro {
                Fixed::ZERO
            } else {
                Fixed::ONE
            },
            boost: Fixed::ZERO,
            speed: settings.speed,
            spread: settings.spread,
            smoke: [Smoke::default(); SMOKE_CAPACITY],
            smoke_head: 0,
            chase: [Fixed64::ZERO; 3],
            cockpit: [Fixed64::ZERO; 3],
            head: [Spring::with_params(
                Fixed::ZERO,
                Fixed::ZERO,
                Fixed::from_int(64),
                Fixed::from_int(11),
            ); 2],
            follow: position,
            camera_yaw: -Fixed::from_ratio(9, 10) * settings.camera_follow,
            chase_yaw: -Fixed::from_ratio(9, 10),
            yaw_diff: -Fixed::from_ratio(9, 10) + Fixed::from_ratio(9, 10) * settings.camera_follow,
            spin_camera: Fixed::ZERO,
            altitude_goal: settings.altitude,
            climb_velocity: Fixed::ZERO,
            climb_tilt: Fixed::ZERO,
            floor: Fixed::ZERO,
            camera_floor: Fixed::ZERO,
            camera_fix: Fixed::ZERO,
            auto_stunt: Fixed::from_int(7),
            smoke_clock: Fixed::ZERO,
        }
    }
    pub fn bytes(&self) -> usize {
        core::mem::size_of::<Self>() + core::mem::size_of_val(&*self.jets)
    }
    pub fn set_wingmen(&mut self, count: usize) {
        self.count = 1 + count.min(2);
        for i in 1..self.count {
            let side = if i == 1 { -Fixed::ONE } else { Fixed::ONE };
            let p = self.jets[0].local(
                [
                    side * (Fixed::from_int((jet::WIDTH) as i32)
                        * Fixed::from_ratio(12, 10)
                        * self.settings.jet_scale
                        + Fixed::from_int(2)),
                    -(Fixed::from_int((jet::LENGTH) as i32))
                        * Fixed::from_ratio(74, 100)
                        * self.settings.jet_scale,
                    Fixed::ZERO,
                ],
                Fixed::ONE,
            );
            self.jets[i] = Aircraft::new(p, self.jets[0].yaw);
        }
    }
    pub fn cycle_view(&mut self) {
        self.view = self.view.next();
    }
    pub fn roll(&mut self) {
        if self.jets[0].stunt >= Fixed::ZERO {
            return;
        }
        self.spin_camera = if self.cockpit[2].to_fixed() > Fixed::from_ratio(5, 10) {
            Fixed::ONE
        } else {
            Fixed::ZERO
        };
        let direction = if self.jets[0].yaw_rate >= Fixed::ZERO {
            Fixed::ONE
        } else {
            -Fixed::ONE
        };
        for (i, j) in self.jets[..self.count].iter_mut().enumerate() {
            j.stunt = Fixed::ZERO;
            j.stunt_direction = direction;
            j.stunt_delay = Fixed::from_int((i) as i32) * Fixed::from_ratio(16, 100);
        }
    }
    pub fn release_pointer(&mut self, cancelled: bool) {
        if !cancelled && self.controls.holding && self.controls.hold_time < Fixed::from_ratio(2, 10)
        {
            self.roll();
        }
        self.controls.holding = false;
    }
    pub fn flame_length(&self) -> Fixed {
        (Fixed::from_ratio(7, 100) + Fixed::from_ratio(17, 100) * self.settings.afterburner)
            * Fixed::from_int((jet::LENGTH) as i32)
            * self.burn
            * (Fixed::from_ratio(16, 10) - self.spread * Fixed::from_ratio(6, 10))
                .clamp(Fixed::ZERO, Fixed::ONE)
            * (Fixed::ONE + Fixed::from_ratio(16, 10) * self.boost)
    }
    pub fn flame_bend(&self) -> Fixed {
        (self.jets[0].yaw_rate / self.settings.agility.max(Fixed::from_ratio(1, 10)))
            .clamp(-Fixed::ONE, Fixed::ONE)
            * Fixed::from_ratio(36, 100)
    }
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
            perspective: Fixed64::ZERO,
            slope: Fixed::ONE,
            scale: viewport_scale,
            center: [
                Fixed::from_int((width / 2) as i32),
                Fixed::from_int((height / 2) as i32),
            ],
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
    pub fn step(&mut self, world: &World, milliseconds: u16, width: usize, height: usize) {
        self.elapsed_ms += u64::from(milliseconds.min(50));
        let now = (Fixed64::from_int(self.elapsed_ms as i64) / 1000).to_fixed();
        let dt = now - self.time;
        if dt == Fixed::ZERO {
            return;
        }
        let s = self.settings;
        let old_camera = self.projection(width, height);
        self.time = now;
        self.intro += dt;
        self.controls.pointer_idle += dt;
        if self.controls.holding {
            self.controls.hold_time += dt;
        }
        let ready = self.intro > Fixed::from_ratio(19, 10);
        follow(
            &mut self.burn,
            if ready { Fixed::ONE } else { Fixed::ZERO },
            Fixed::from_int(5),
            dt,
        );
        let pushing = ready
            && ((self.controls.holding && self.controls.hold_time >= Fixed::from_ratio(2, 10))
                || self.controls.keys & 768 != 0);
        follow(
            &mut self.boost,
            if pushing { Fixed::ONE } else { Fixed::ZERO },
            if pushing {
                Fixed::from_int(5)
            } else {
                Fixed::from_ratio(24, 10)
            },
            dt,
        );
        if !pushing && self.boost < Fixed::from_ratio(5, 10000) {
            self.boost = Fixed::ZERO;
        }
        self.speed = s.speed
            * (Fixed::ONE
                + (s.boost.clamp(Fixed::ONE, Fixed::from_int(4)) - Fixed::ONE) * self.boost);
        follow(&mut self.spread, s.spread, Fixed::from_int(11), dt);
        let j = &self.jets[0];
        let rate = s.agility.max(Fixed::from_ratio(1, 10));
        let clearance = s.clearance.clamp(Fixed::from_int(2), Fixed::from_int(60));
        let mut ground: u8 = 0;
        for i in 0..=4 {
            for side in -1..=1 {
                let distance = self.speed * Fixed::from_ratio(16, 100) * Fixed::from_int(i);
                ground = ground.max(
                    world.height(
                        (j.position[0]
                            + j.yaw.sin() * distance
                            + j.yaw.cos() * Fixed::from_int(side) * Fixed::from_int(7))
                        .trunc_to_int(),
                        (j.position[1] - j.yaw.cos() * distance
                            + j.yaw.sin() * Fixed::from_int(side) * Fixed::from_int(7))
                        .trunc_to_int(),
                    ),
                );
            }
        }
        let floor = Fixed::from_int((ground) as i32) + clearance;
        if self.floor == Fixed::ZERO {
            self.floor = floor;
        }
        let floor_rate = if floor > self.floor {
            Fixed::from_int(9)
        } else {
            Fixed::from_int(3)
        };
        follow(&mut self.floor, floor, floor_rate, dt);
        let ceiling = self
            .floor
            .max(s.altitude)
            .max(s.ceiling.clamp(Fixed::from_int(20), Fixed::from_int(900)));
        let climb_max = self.speed * Fixed::from_ratio(55, 100);
        let chase =
            self.chase[2].to_fixed() + self.cockpit[2].to_fixed() > Fixed::from_ratio(5, 10);
        let mut flown = false;
        let mut want = Fixed::ZERO;
        if ready {
            if self.controls.keyboard {
                let bits = self.controls.keys;
                want = Fixed::from_int(i32::from(bits & 12 != 0) - i32::from(bits & 3 != 0)) * rate;
                self.altitude_goal +=
                    Fixed::from_int(i32::from(bits & 48 != 0) - i32::from(bits & 192 != 0))
                        * climb_max
                        * dt;
                flown = true;
            } else if let Some(pointer) = self
                .controls
                .pointer
                .filter(|_| self.controls.pointer_idle < Fixed::from_int(5))
            {
                if chase {
                    let turn = ((pointer[0] - old_camera.center[0])
                        / (Fixed::from_int((width) as i32) * Fixed::from_ratio(26, 100)))
                    .clamp(-Fixed::ONE, Fixed::ONE);
                    if turn.abs() > Fixed::from_ratio(6, 100) {
                        want = turn * rate;
                    }
                    let y = old_camera.steer_y;
                    let raw = if pointer[1] < y {
                        (y - pointer[1]) / y.max(Fixed::ONE)
                    } else {
                        -(pointer[1] - y) / (Fixed::from_int((height) as i32) - y).max(Fixed::ONE)
                    };
                    let up = raw.signum()
                        * (raw.abs() - Fixed::from_ratio(8, 100)).max(Fixed::ZERO)
                        / Fixed::from_ratio(92, 100);
                    self.altitude_goal = if up >= Fixed::ZERO {
                        s.altitude + up * (ceiling - s.altitude)
                    } else {
                        s.altitude + up * (s.altitude - self.floor)
                    };
                    flown = true;
                } else if let Some(point) = old_camera.project(j.position) {
                    let dx = pointer[0] - point[0];
                    let dy = pointer[1] - point[1];
                    if dx * dx + dy * dy > Fixed::from_int(324) {
                        let (sn, cs) = old_camera.yaw.sin_cos();
                        want =
                            (wrap_angle((dx * cs + dy * sn).atan2(-(-dx * sn + dy * cs)) - j.yaw)
                                * Fixed::from_ratio(26, 10))
                            .clamp(-rate, rate);
                    }
                    flown = true;
                }
            } else {
                want = (rate
                    * (Fixed::from_ratio(62, 100)
                        * (self.time * Fixed::from_ratio(31, 100) + Fixed::from_ratio(13, 10))
                            .sin()
                        + Fixed::from_ratio(38, 100)
                            * (self.time * Fixed::from_ratio(73, 100) + Fixed::from_ratio(4, 10))
                                .sin()
                        + Fixed::from_ratio(22, 100)
                            * (self.time * Fixed::from_ratio(17, 100)).sin()))
                .clamp(-rate, rate);
                if s.stunts {
                    self.auto_stunt -= dt;
                    if self.auto_stunt <= Fixed::ZERO {
                        self.roll();
                        self.auto_stunt = Fixed::from_int(9)
                            + Fixed::from_int(
                                (super::world::hash8(
                                    (self.time * Fixed::from_int(10)).trunc_to_int() as u32,
                                )) as i32,
                            ) / Fixed::from_int(255)
                                * Fixed::from_int(7);
                    }
                }
            }
        }
        let j = &mut self.jets[0];
        follow(&mut j.yaw_rate, want, Fixed::from_ratio(26, 10), dt);
        j.yaw += j.yaw_rate * dt;
        follow(
            &mut j.roll,
            (j.yaw_rate / rate).clamp(-Fixed::ONE, Fixed::ONE) * s.bank * PI / Fixed::from_int(180),
            Fixed::from_int(5),
            dt,
        );
        j.velocity = [j.yaw.sin() * self.speed, -j.yaw.cos() * self.speed];
        j.position[0] += j.velocity[0] * dt;
        j.position[1] += j.velocity[1] * dt;
        if !flown {
            follow(
                &mut self.altitude_goal,
                s.altitude,
                Fixed::from_ratio(9, 10),
                dt,
            );
        }
        self.altitude_goal = self.altitude_goal.clamp(self.floor, ceiling);
        let dz = ((self.altitude_goal
            + (self.time * Fixed::from_ratio(9, 10)).sin() * Fixed::from_ratio(15, 10)
            - j.position[2])
            * (Fixed::ONE - (-dt * Fixed::from_int(3)).exp()))
        .clamp(-climb_max * dt, climb_max * dt);
        j.position[2] += dz;
        let hard = Fixed::from_int(
            (world.height(
                (j.position[0]).trunc_to_int(),
                (j.position[1]).trunc_to_int(),
            )) as i32,
        ) + clearance * Fixed::from_ratio(7, 10);
        if j.position[2] < hard {
            follow(&mut j.position[2], hard, Fixed::from_int(22), dt);
        }
        follow(
            &mut self.climb_velocity,
            dz / dt.max(Fixed::from_ratio(1, 10000)),
            Fixed::from_int(6),
            dt,
        );
        self.climb_tilt = (self.climb_velocity.signum()
            * (self.climb_velocity.abs() - Fixed::from_int(3)).max(Fixed::ZERO)
            / self.speed.max(Fixed::from_int(20))
            * Fixed::from_ratio(125, 100))
        .clamp(-Fixed::from_ratio(5, 10), Fixed::from_ratio(5, 10));
        follow(
            &mut j.pitch,
            (j.yaw_rate / rate).abs() * Fixed::from_ratio(1, 10) + self.climb_tilt,
            Fixed::from_int(4),
            dt,
        );
        let goals = [
            (j.yaw_rate / rate).clamp(-Fixed::ONE, Fixed::ONE) * Fixed::from_ratio(64, 10)
                + (self.time * Fixed::from_ratio(53, 100)).sin() * Fixed::from_ratio(3, 10),
            (self.climb_tilt / Fixed::from_ratio(5, 10)).clamp(-Fixed::ONE, Fixed::ONE)
                * Fixed::from_ratio(34, 10)
                + self.boost * Fixed::from_ratio(16, 10)
                + (self.time * Fixed::from_ratio(71, 100) + Fixed::ONE).sin()
                    * Fixed::from_ratio(25, 100),
        ];
        for (spring, goal) in self.head.iter_mut().zip(goals) {
            spring.retarget(goal, None);
            spring.tick((dt * Fixed::from_int(1000)).round().to_int() as u16);
        }
        let position = j.position;
        let yaw = j.yaw;
        let velocity = j.velocity;
        let roll = j.roll;
        let pitch = j.pitch;
        let yaw_rate = j.yaw_rate;
        for i in 1..self.count {
            let j = &mut self.jets[i];
            let side = if i == 1 { -Fixed::ONE } else { Fixed::ONE };
            let x = side
                * (Fixed::from_int((jet::WIDTH) as i32) * Fixed::from_ratio(12, 10) * s.jet_scale
                    + Fixed::from_int(2));
            let y =
                Fixed::from_int((jet::LENGTH) as i32) * Fixed::from_ratio(74, 100) * s.jet_scale;
            let target = [
                position[0] + yaw.cos() * x - yaw.sin() * y,
                position[1] + yaw.sin() * x + yaw.cos() * y,
            ];
            for k in 0..2 {
                let old = j.position[k];
                j.position[k] += velocity[k] * dt;
                follow(&mut j.position[k], target[k], Fixed::from_ratio(32, 10), dt);
                j.velocity[k] = (j.position[k] - old) / dt;
            }
            j.yaw += wrap_angle(yaw - j.yaw) * (Fixed::ONE - (-dt * Fixed::from_int(4)).exp());
            follow(&mut j.roll, roll, Fixed::from_ratio(32, 10), dt);
            j.pitch = pitch;
            follow(
                &mut j.position[2],
                position[2]
                    + (self.time * Fixed::from_ratio(11, 10)
                        + Fixed::from_int((i) as i32) * Fixed::from_int(2))
                    .sin()
                        * Fixed::from_ratio(12, 10),
                Fixed::from_int(3),
                dt,
            );
            j.position[2] = j.position[2].max(
                Fixed::from_int(
                    (world.height(
                        (j.position[0]).trunc_to_int(),
                        (j.position[1]).trunc_to_int(),
                    )) as i32,
                ) + clearance * Fixed::from_ratio(7, 10),
            );
        }
        for j in &mut self.jets[..self.count] {
            let load = (yaw_rate.abs() / rate
                + if j.stunt >= Fixed::ZERO {
                    Fixed::from_ratio(7, 10)
                } else {
                    Fixed::ZERO
                }
                + self.boost * Fixed::from_ratio(9, 10)
                + self.climb_tilt.abs())
            .clamp(Fixed::ZERO, Fixed::ONE);
            follow(&mut j.load, load, Fixed::from_ratio(25, 10), dt);
            if j.stunt >= Fixed::ZERO {
                if j.stunt_delay > Fixed::ZERO {
                    j.stunt_delay -= dt;
                } else {
                    j.stunt += dt / Fixed::from_ratio(85, 100);
                    if j.stunt >= Fixed::ONE {
                        j.stunt = -Fixed::ONE;
                    }
                }
            }
            if (j.position[0] - j.last_trail[0]).powi(2) + (j.position[1] - j.last_trail[1]).powi(2)
                > Fixed::from_ratio(625, 100)
            {
                j.last_trail = [j.position[0], j.position[1]];
                for w in 0..2 {
                    j.trails[w][j.trail_head] = TrailPoint {
                        position: j.local(
                            [
                                if w == 0 {
                                    -Fixed::from_ratio(155, 10)
                                } else {
                                    Fixed::from_ratio(155, 10)
                                },
                                jet::PIVOT[0] - Fixed::from_int(33),
                                Fixed::from_int(4) - jet::PIVOT[1],
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
        if self.smoke_clock
            >= Fixed::from_ratio(34, 1000) - Fixed::from_ratio(12, 1000) * self.boost
            && self.burn > Fixed::from_ratio(6, 10)
            && s.afterburner > Fixed::from_ratio(5, 100)
            && self.spread < Fixed::from_int(2)
        {
            self.smoke_clock = Fixed::ZERO;
            let length = self.flame_length();
            let bend = self.flame_bend();
            let t = length * Fixed::from_ratio(78, 100);
            for (i, j) in self.jets[..self.count].iter().enumerate() {
                let head = self.smoke_head;
                let side = if (head + i) & 1 != 0 {
                    Fixed::ONE
                } else {
                    -Fixed::ONE
                };
                self.smoke[head] = Smoke {
                    position: j.local(
                        [
                            side * Fixed::from_ratio(196, 100)
                                + bend * t * t / length.max(Fixed::ONE),
                            jet::PIVOT[0] - Fixed::from_ratio(436, 10) - t,
                            Fixed::from_ratio(41, 10) - jet::PIVOT[1],
                        ],
                        s.jet_scale,
                    ),
                    born: self.time,
                    size: (Fixed::from_ratio(13, 10)
                        + Fixed::from_int((super::world::hash8((head * 31 + 7) as u32)) as i32)
                            / Fixed::from_int(255)
                            * Fixed::from_ratio(12, 10))
                        * s.jet_scale,
                    push: [
                        -velocity[0] * Fixed::from_ratio(3, 10)
                            + (Fixed::from_int(
                                (super::world::hash8((head * 17 + 3) as u32)) as i32,
                            ) - Fixed::from_int(128))
                                * Fixed::from_ratio(5, 100),
                        -velocity[1] * Fixed::from_ratio(3, 10)
                            + (Fixed::from_int(
                                (super::world::hash8((head * 13 + 5) as u32)) as i32,
                            ) - Fixed::from_int(128))
                                * Fixed::from_ratio(5, 100),
                    ],
                    strength: (Fixed::from_ratio(46, 100)
                        + Fixed::from_ratio(36, 100) * self.boost)
                        * (s.afterburner * Fixed::from_ratio(14, 10))
                            .clamp(Fixed::ZERO, Fixed::ONE),
                    life: Fixed::from_ratio(75, 100)
                        + Fixed::from_ratio(5, 10) * self.boost
                        + Fixed::from_int((super::world::hash8((head * 5) as u32)) as i32)
                            / Fixed::from_int(255)
                            * Fixed::from_ratio(3, 10),
                };
                self.smoke_head = (head + 1) % SMOKE_CAPACITY;
            }
        }
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
        let look = (Fixed::from_int(22) + Fixed::from_int(16) * self.boost)
            * (Fixed::ONE - self.chase[2].to_fixed() - self.cockpit[2].to_fixed());
        for (k, target) in [
            position[0] + yaw.sin() * look,
            position[1] - yaw.cos() * look,
        ]
        .into_iter()
        .enumerate()
        {
            self.follow[k] += velocity[k] * dt;
            follow(&mut self.follow[k], target, Fixed::from_ratio(35, 10), dt);
        }
        follow(
            &mut self.camera_yaw,
            -yaw * s.camera_follow.clamp(Fixed::ZERO, Fixed::ONE),
            Fixed::from_ratio(16, 10),
            dt,
        );
        follow(&mut self.follow[2], position[2], Fixed::from_int(3), dt);
        let mut fix = Fixed::ZERO;
        self.camera_floor = Fixed::ZERO;
        if self.chase[2].to_fixed() > Fixed::from_ratio(3, 10)
            && old_camera.perspective > Fixed64::from_ratio(1, 10000)
        {
            let back = old_camera.divide_perspective(Fixed::ONE);
            let (sn, cs) = old_camera.yaw.sin_cos();
            let x = old_camera.position[0] + sn * back;
            let y = old_camera.position[1] + cs * back;
            let mut high = 0;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    high = high.max(world.height(
                        (x + Fixed::from_int(dx) * Fixed::from_int(9)).trunc_to_int(),
                        (y + Fixed::from_int(dy) * Fixed::from_int(9)).trunc_to_int(),
                    ));
                }
            }
            self.camera_floor = Fixed::from_int((high) as i32) + Fixed::from_int(5);
            let nominal = self.follow[2] + old_camera.slope * back;
            let mut wanted = nominal.max(self.camera_floor);
            if wanted > CLOUD_ALT - Fixed::from_int(3)
                && wanted < Fixed::from_int((MAX_HEIGHT) as i32) + Fixed::from_int(3)
            {
                wanted = if position[2] < CLOUD_ALT - Fixed::from_int(8) {
                    self.camera_floor.max(CLOUD_ALT - Fixed::from_int(3))
                } else {
                    Fixed::from_int((MAX_HEIGHT) as i32) + Fixed::from_int(3)
                };
            }
            fix = wanted - nominal;
        }
        follow(&mut self.camera_fix, fix, Fixed::from_int(7), dt);
        follow(&mut self.chase_yaw, -yaw, Fixed::from_ratio(34, 10), dt);
        let raw = self.chase_yaw - self.camera_yaw;
        if self.chase[2].to_fixed() == Fixed::ZERO && self.cockpit[2].to_fixed() == Fixed::ZERO {
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
        let world = World::new(7, Fixed::from_ratio(33, 100));
        let mut flight = Flight::new(&world, Settings::default());
        for target in [View::Chase, View::Cockpit, View::Overhead, View::Cockpit] {
            let before = flight.projection(427, 240);
            flight.view = target;
            assert_eq!(before.center, flight.projection(427, 240).center);
            for _ in 0..400 {
                flight.step(&world, 16, 427, 240);
                let p = flight.projection(427, 240);
                assert!(
                    p.center
                        .into_iter()
                        .chain(p.position)
                        .all(|v| v.abs() < Fixed::from_int(1_000_000))
                );
            }
            assert_eq!(
                flight.cockpit[2].to_fixed(),
                Fixed::from_int((target == View::Cockpit) as i32)
            );
            assert_eq!(
                flight.chase[2].to_fixed(),
                Fixed::from_int((target == View::Chase) as i32)
            );
        }
        for i in 0..300 {
            flight.view = if i % 2 == 0 {
                View::Chase
            } else {
                View::Cockpit
            };
            flight.step(&world, 16, 427, 240);
            assert!(flight.chase[2] + flight.cockpit[2] <= Fixed64::from_ratio(100001, 100000));
        }
    }
    #[test]
    fn controls_cancel_and_history_storage_remain_bounded() {
        // Model moves must not carry the three fixed aircraft history buffers.
        assert!(core::mem::size_of::<Flight>() < 8 * 1024);
        let world = World::new(7, Fixed::from_ratio(33, 100));
        let mut flight = Flight::new(&world, Settings::default());
        flight.set_wingmen(2);
        flight.controls.key(0, true);
        flight.controls.key(1, true);
        flight.controls.key(0, false);
        assert_eq!(flight.controls.keys, 2);
        flight.controls.press();
        flight.release_pointer(true);
        assert!(flight.jets[0].stunt < Fixed::from_ratio(0, 1));
        flight.controls.press();
        flight.release_pointer(false);
        assert_eq!(flight.jets[0].stunt, Fixed::from_ratio(0, 1));
        flight.controls.release_all();
        assert_eq!(flight.controls.keys, 0);
        let allocations = crate::test_support::tracked_allocations(|| {
            for _ in 0..2000 {
                flight.step(&world, 16, 427, 240);
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(flight.time, Fixed::from_int(32));
        for jet in flight.jets.iter() {
            assert!(jet.trail_len <= TRAIL_CAPACITY);
            assert!(jet.trail_head < TRAIL_CAPACITY);
            assert!(
                jet.position
                    .iter()
                    .all(|v| v.abs() < Fixed::from_int(1_000_000))
            );
        }
        assert!(flight.smoke_head < SMOKE_CAPACITY);
    }
}
