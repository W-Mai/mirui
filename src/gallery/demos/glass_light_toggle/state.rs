use super::icons::{COUNT, PAIRS};
use crate::anim::{Spring, SpringConfig};
use crate::types::{Color, Fixed, Point};

pub(super) const TRAVEL: i32 = 208;
const SETTLE: SpringConfig = SpringConfig {
    duration_ms: 720,
    bounce: Fixed::from_ratio(1, 5),
};
const LIGHT: SpringConfig = SpringConfig {
    duration_ms: 560,
    bounce: Fixed::ZERO,
};

pub(super) fn smooth(value: Fixed) -> Fixed {
    let t = value.clamp(Fixed::ZERO, Fixed::ONE);
    t * t * (Fixed::from_int(3) - Fixed::from_int(2) * t)
}

#[derive(Clone, Copy)]
pub(super) struct Pixel {
    pub position: Point,
    pub alpha: u8,
    pub size: Fixed,
}
#[derive(Clone, Copy)]
pub(super) struct Scene {
    pub progress: Fixed,
    pub velocity: Fixed,
    pub light: Fixed,
    pub light_direction: i8,
    pub press: Fixed,
    pub pixels: [Pixel; COUNT],
}
#[derive(Clone, Copy)]
struct Particle {
    x: Spring,
    y: Spring,
    alpha: u8,
    size: Fixed,
}

impl Particle {
    /// At rest where switch progress `p` puts it.
    fn at(i: usize, p: Fixed) -> Self {
        let goal = particle_goal(i, p);
        let stiffness = Fixed::from_int(280 + (i as i32 * 37) % 160);
        let damping = Fixed::from_int(29);
        Self {
            x: Spring::with_params(goal.position.x, goal.position.x, stiffness, damping),
            y: Spring::with_params(goal.position.y, goal.position.y, stiffness, damping),
            alpha: goal.alpha,
            size: goal.size,
        }
    }
}

/// Where particle `i` heads for switch progress `p`: its moon cell at 0,
/// its sun cell at 1, along a staggered arc in between.
fn particle_goal(i: usize, p: Fixed) -> Pixel {
    let pair = PAIRS[i];
    let delay = Fixed::from_ratio(4, 100)
        + Fixed::from_ratio(8 - i32::from(pair.moon.x), 16) * Fixed::from_ratio(26, 100)
        + Fixed::from_ratio((i as i32 * 17) % 6, 100);
    let t = smooth((p - delay) / Fixed::from_ratio(62, 100));
    let arc = Fixed::sin_deg(t * Fixed::from_int(180));
    let a = Point::new(
        Fixed::from_ratio(i32::from(pair.moon.x) * 57, 10),
        Fixed::from_ratio(i32::from(pair.moon.y) * 57, 10),
    );
    let b = Point::new(
        Fixed::from_ratio(i32::from(pair.sun.x) * 57, 10),
        Fixed::from_ratio(i32::from(pair.sun.y) * 57, 10),
    );
    let curl = Fixed::from_int((i as i32 * 29) % 33 - 16);
    Pixel {
        position: Point::new(
            a.x + (b.x - a.x) * t + arc * curl * Fixed::from_ratio(9, 10),
            a.y + (b.y - a.y) * t - arc * (Fixed::from_int(4) + curl.abs() / 2),
        ),
        alpha: (Fixed::from_int(i32::from(pair.moon.alpha))
            + (Fixed::from_int(i32::from(pair.sun.alpha))
                - Fixed::from_int(i32::from(pair.moon.alpha)))
                * t)
            .to_int()
            .clamp(0, 255) as u8,
        size: Fixed::from_ratio(46, 10) * (Fixed::ONE - arc * Fixed::from_ratio(45, 100)),
    }
}

#[crate::model]
pub(super) struct GlassModel {
    position: Spring,
    light: Spring,
    light_direction: i8,
    press: Spring,
    particles: [Particle; COUNT],
    dragging: bool,
    accepted: bool,
    drag_origin: Fixed,
    drag_start: Fixed,
    revision: u32,
    /// Whether the host theme was light when last read; `None` before the first frame.
    host_day: Option<bool>,
}
impl Default for GlassModel {
    fn default() -> Self {
        Self {
            position: Spring::preset(Fixed::ZERO, Fixed::ZERO, SETTLE),
            light: Spring::preset(Fixed::ZERO, Fixed::ZERO, LIGHT),
            light_direction: 1,
            press: Spring::preset(Fixed::ZERO, Fixed::ZERO, LIGHT),
            particles: core::array::from_fn(|i| Particle::at(i, Fixed::ZERO)),
            dragging: false,
            accepted: false,
            drag_origin: Fixed::ZERO,
            drag_start: Fixed::ZERO,
            revision: 0,
            host_day: None,
        }
    }
}
#[crate::model]
impl GlassModel {
    #[observe]
    pub(super) fn visual_revision(&self) -> u32 {
        self.revision
    }
    #[observe]
    pub(super) fn hint_color(&self) -> Color {
        Color::rgb(83, 83, 86).blend_with(
            Color::rgb(154, 152, 149),
            smooth(
                (self.position.position - Fixed::from_ratio(18, 100)) / Fixed::from_ratio(64, 100),
            ),
        )
    }
    pub(super) fn scene(&self) -> Scene {
        Scene {
            progress: self.position.position,
            velocity: self.position.velocity,
            light: self.light.position,
            light_direction: self.light_direction,
            press: self.press.position,
            pixels: core::array::from_fn(|i| {
                let p = self.particles[i];
                Pixel {
                    position: Point::new(p.x.position, p.y.position),
                    alpha: p.alpha,
                    size: p.size,
                }
            }),
        }
    }
    pub(super) fn active(&self) -> bool {
        self.dragging
            || !self.position.is_settled()
            || !self.light.is_settled()
            || !self.press.is_settled()
            || self
                .particles
                .iter()
                .any(|p| !p.x.is_settled() || !p.y.is_settled())
    }
    pub(super) fn toggle(&mut self) {
        let target = if self.position.target > Fixed::HALF {
            Fixed::ZERO
        } else {
            Fixed::ONE
        };
        self.position.retarget(target, None);
        self.revision = self.revision.wrapping_add(1);
    }
    pub(super) fn choose(&mut self, day: bool) {
        self.position
            .retarget(if day { Fixed::ONE } else { Fixed::ZERO }, None);
        self.revision = self.revision.wrapping_add(1);
    }
    pub(super) fn host_day(&self) -> Option<bool> {
        self.host_day
    }
    /// Follows the host theme. The first reading puts an untouched switch on
    /// that side at rest; a later change flips it the way a tap does.
    pub(super) fn follow_host(&mut self, day: bool) {
        let first = self.host_day.is_none();
        self.host_day = Some(day);
        if !first {
            self.choose(day);
        } else if day && self.revision == 0 {
            self.position = Spring::preset(Fixed::ONE, Fixed::ONE, SETTLE);
            self.light = Spring::preset(Fixed::ONE, Fixed::ONE, LIGHT);
            self.light_direction = -1;
            self.particles = core::array::from_fn(|i| Particle::at(i, Fixed::ONE));
            self.revision = 1;
        }
    }
    pub(super) fn knob_contains(&self, point: Point) -> bool {
        let x = (self.position.position.clamp(Fixed::ZERO, Fixed::ONE) - Fixed::HALF)
            * Fixed::from_int(TRAVEL);
        let dx = ((point.x - x).abs() - Fixed::from_int(41)).max(Fixed::ZERO);
        let dy = (point.y.abs() - Fixed::from_int(41)).max(Fixed::ZERO);
        dx * dx + dy * dy <= Fixed::from_int(58 * 58)
    }
    pub(super) fn pressed(&mut self, inside: bool, draggable: bool) {
        self.accepted = inside && draggable;
        self.press
            .retarget(if inside { Fixed::ONE } else { Fixed::ZERO }, None);
    }
    pub(super) fn release_press(&mut self) {
        self.press.retarget(Fixed::ZERO, None);
    }
    pub(super) fn is_dragging(&self) -> bool {
        self.dragging
    }
    pub(super) fn begin_drag(&mut self, x: Fixed) {
        if !self.accepted {
            return;
        }
        self.dragging = true;
        self.press.retarget(Fixed::ONE, None);
        self.drag_origin = x;
        self.drag_start = self.position.position;
    }
    pub(super) fn drag(&mut self, x: Fixed) {
        if !self.dragging {
            return;
        }
        let p = self.drag_start + (x - self.drag_origin) / Fixed::from_int(TRAVEL);
        let p = if p < Fixed::ZERO {
            p / 5
        } else if p > Fixed::ONE {
            Fixed::ONE + (p - Fixed::ONE) / 5
        } else {
            p
        };
        self.position = Spring::preset(p, p, SETTLE);
        self.revision = self.revision.wrapping_add(1);
    }
    pub(super) fn end_drag(&mut self, velocity: Fixed, cancelled: bool) {
        if !self.dragging {
            self.release_press();
            return;
        }
        self.dragging = false;
        let goal = if cancelled {
            self.drag_start > Fixed::HALF
        } else {
            self.position.position + velocity / Fixed::from_int(TRAVEL) * Fixed::from_ratio(15, 100)
                > Fixed::HALF
        };
        self.position.set_velocity(if cancelled {
            Fixed::ZERO
        } else {
            velocity / Fixed::from_int(TRAVEL)
        });
        self.position
            .retarget(if goal { Fixed::ONE } else { Fixed::ZERO }, None);
        self.release_press();
        self.revision = self.revision.wrapping_add(1);
    }
    pub(super) fn advance(&mut self, delta: u16) {
        if delta == 0 || !self.active() {
            return;
        }
        if !self.dragging {
            self.position.tick(delta);
        }
        let p = self.position.position.clamp(Fixed::ZERO, Fixed::ONE);
        self.light.retarget(smooth(p), None);
        self.light.tick(delta);
        if self.light.position < Fixed::from_ratio(15, 1000) {
            self.light_direction = 1;
        } else if self.light.position > Fixed::from_ratio(985, 1000) {
            self.light_direction = -1;
        }
        self.press.tick(delta);
        for (i, particle) in self.particles.iter_mut().enumerate() {
            let goal = particle_goal(i, p);
            particle.x.retarget(goal.position.x, None);
            particle.y.retarget(goal.position.y, None);
            particle.x.tick(delta);
            particle.y.tick(delta);
            particle.alpha = goal.alpha;
            particle.size = goal.size;
        }
        self.revision = self.revision.wrapping_add(1);
    }
}
