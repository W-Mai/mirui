#![doc = " Flight dynamics and a single camera shared by all three views."]
use super::math::FixedMath as _;
use super::math::TAU;
use super::{jet, world::World};
use alloc::{boxed::Box, vec::Vec};
use mirui::{
    anim::Spring,
    types::{Fixed, Fixed64},
};
mod camera;
mod simulation;
pub use camera::Projection;

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
        aircraft_axes([self.yaw, self.pitch, self.roll + self.spin()])
    }
    pub fn local(&self, local: V3, scale: Fixed) -> V3 {
        aircraft_point(self.position, self.axes(), local, scale)
    }
}
pub(crate) fn aircraft_axes([yaw, pitch, roll]: V3) -> [V3; 3] {
    let (sn, cs) = yaw.sin_cos();
    let (sr, cr) = roll.sin_cos();
    let (sp, cp) = pitch.sin_cos();
    [
        [cs * cr, sn * cr, -sr],
        [sn * cp + cs * sr * sp, -cs * cp + sn * sr * sp, cr * sp],
        [cs * sr * cp - sn * sp, sn * sr * cp + cs * sp, cr * cp],
    ]
}
pub(crate) fn aircraft_point(position: V3, axes: [V3; 3], local: V3, scale: Fixed) -> V3 {
    let [right, forward, up] = axes;
    core::array::from_fn(|i| {
        position[i] + scale * (local[0] * right[i] + local[1] * forward[i] + local[2] * up[i])
    })
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
    pub fn step(&mut self, world: &World, milliseconds: u16, width: usize, height: usize) {
        self.step_above(
            world.at(Some(Default::default())),
            milliseconds,
            width,
            height,
        );
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
