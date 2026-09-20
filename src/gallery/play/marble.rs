use super::change::ChangeSet;
use super::clock::BoundedClock;
use super::input::DragTransaction;
use crate::types::Fixed64;

pub(crate) const MAX_BALLS: usize = 8;
pub(crate) const MAX_PADS: usize = 6;
pub(crate) const MAX_PARTICLES: usize = 24;
pub(crate) const MAX_RINGS: usize = 8;
pub(crate) const TRAIL_LEN: usize = 6;

pub(crate) const PALETTE: [u32; 6] = [0xb4eabd, 0xc6b0ef, 0xeed984, 0xeeac8b, 0xa0d2e8, 0xd5eba0];

const STEP: Fixed64 = Fixed64::from_ratio(1, 120);
const RAILS: [(i64, i64, i64, i64); 4] = [
    (36, 145, 78, 165),
    (215, 80, 264, 70),
    (269, 213, 308, 234),
    (404, 144, 437, 122),
];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Vec2 {
    pub(crate) x: Fixed64,
    pub(crate) y: Fixed64,
}

impl Vec2 {
    pub(crate) const fn new(x: i64, y: i64) -> Self {
        Self {
            x: Fixed64::from_int(x),
            y: Fixed64::from_int(y),
        }
    }

    const fn fixed(x: Fixed64, y: Fixed64) -> Self {
        Self { x, y }
    }

    fn length(self) -> Fixed64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Page {
    Play,
    Edit,
    Scenes,
    Settings,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Theme {
    pub(crate) name: &'static str,
    pub(crate) subtitle: &'static str,
    pub(crate) bg: u32,
    pub(crate) panel: u32,
    pub(crate) accent: u32,
    pub(crate) gravity: Fixed64,
}

pub(crate) const THEMES: [Theme; 3] = [
    Theme {
        name: "DAYDREAM",
        subtitle: "SOFT GREEN / NORMAL",
        bg: 0x17221c,
        panel: 0x222f25,
        accent: 0xd9f88a,
        gravity: Fixed64::from_ratio(80, 100),
    },
    Theme {
        name: "AFTER HOURS",
        subtitle: "BLUE GREY / LIGHT",
        bg: 0x1b2029,
        panel: 0x29303b,
        accent: 0xcfbbee,
        gravity: Fixed64::from_ratio(45, 100),
    },
    Theme {
        name: "ZERO GRAVITY",
        subtitle: "COOL BLUE / FLOAT",
        bg: 0x17242a,
        panel: 0x22343b,
        accent: 0xa9dfeb,
        gravity: Fixed64::from_ratio(8, 100),
    },
];

#[derive(Clone, Copy, Debug)]
pub(crate) struct Pad {
    pub(crate) id: u32,
    pub(crate) letter: u8,
    pub(crate) pos: Vec2,
    pub(crate) radius: Fixed64,
    pub(crate) color: u32,
    pub(crate) bounce: Fixed64,
    pub(crate) pulse: Fixed64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Ball {
    pub(crate) pos: Vec2,
    pub(crate) velocity: Vec2,
    pub(crate) color: u32,
    pub(crate) trail: [Vec2; TRAIL_LEN],
    pub(crate) trail_len: usize,
    trail_clock: Fixed64,
    cooldown: [Fixed64; MAX_PADS],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Ring {
    pub(crate) pos: Vec2,
    pub(crate) radius: Fixed64,
    pub(crate) age: Fixed64,
    pub(crate) color: u32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Particle {
    pub(crate) pos: Vec2,
    pub(crate) velocity: Vec2,
    pub(crate) age: Fixed64,
    pub(crate) color: u32,
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    slot: Option<usize>,
    start: Vec2,
    offset: Vec2,
    position: Option<DragTransaction<Vec2>>,
}

#[derive(Debug)]
pub(crate) struct MarbleModel {
    pub(crate) page: Page,
    pub(crate) paused: bool,
    pub(crate) scene: usize,
    pub(crate) gravity: Fixed64,
    pub(crate) trails: bool,
    pub(crate) feedback: bool,
    pub(crate) inspector: bool,
    pub(crate) add_mode: bool,
    pub(crate) pads: [Option<Pad>; MAX_PADS],
    pub(crate) balls: [Option<Ball>; MAX_BALLS],
    pub(crate) rings: [Option<Ring>; MAX_RINGS],
    pub(crate) particles: [Option<Particle>; MAX_PARTICLES],
    pub(crate) selected: usize,
    pub(crate) hits: u32,
    pub(crate) flash: Fixed64,
    pub(crate) tilt: Vec2,
    pub(crate) target: Vec2,
    pub(crate) toast: &'static str,
    pub(crate) toast_left: Fixed64,
    sim_time: Fixed64,
    next_id: u32,
    rng: u32,
    keys: u8,
    drag: Option<Drag>,
    clock: BoundedClock,
    ring_cursor: usize,
    particle_cursor: usize,
}

impl Default for MarbleModel {
    fn default() -> Self {
        Self::new()
    }
}

impl MarbleModel {
    pub(crate) fn new() -> Self {
        let mut model = Self {
            page: Page::Play,
            paused: false,
            scene: 0,
            gravity: THEMES[0].gravity,
            trails: true,
            feedback: true,
            inspector: false,
            add_mode: false,
            pads: [None; MAX_PADS],
            balls: [None; MAX_BALLS],
            rings: [None; MAX_RINGS],
            particles: [None; MAX_PARTICLES],
            selected: 0,
            hits: 0,
            flash: Fixed64::ZERO,
            tilt: Vec2::default(),
            target: Vec2::default(),
            toast: "",
            toast_left: Fixed64::ZERO,
            sim_time: Fixed64::ZERO,
            next_id: 6,
            rng: 0x2a71_b893,
            keys: 0,
            drag: None,
            clock: BoundedClock::new(120, 60, 8),
            ring_cursor: 0,
            particle_cursor: 0,
        };
        model.reset_scene(0);
        model.toast = "";
        model.toast_left = Fixed64::ZERO;
        model
    }

    pub(crate) fn theme(&self) -> Theme {
        THEMES[self.scene]
    }

    pub(crate) fn pad_count(&self) -> usize {
        self.pads.iter().flatten().count()
    }

    pub(crate) fn ball_count(&self) -> usize {
        self.balls.iter().flatten().count()
    }

    pub(crate) fn selected_pad(&self) -> &Pad {
        self.pads[self.selected]
            .as_ref()
            .expect("a marble scene always owns at least one pad")
    }

    pub(crate) fn running(&self) -> bool {
        self.page == Page::Play && !self.paused
    }

    pub(crate) fn needs_animation(&self) -> bool {
        self.running()
            || self.toast_left.is_positive()
            || self.flash.is_positive()
            || self
                .pads
                .iter()
                .flatten()
                .any(|pad| pad.pulse.is_positive())
            || self.rings.iter().any(Option::is_some)
            || self.particles.iter().any(Option::is_some)
    }

    fn notify(&mut self, message: &'static str) {
        self.toast = message;
        self.toast_left = Fixed64::from_ratio(17, 10);
    }

    fn random(&mut self) -> u32 {
        let mut value = self.rng;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.rng = value;
        value
    }

    pub(crate) fn reset_scene(&mut self, index: usize) -> ChangeSet {
        self.cancel_input();
        self.scene = index.min(THEMES.len() - 1);
        self.gravity = self.theme().gravity;
        self.pads = [None; MAX_PADS];
        self.balls = [None; MAX_BALLS];
        self.rings = [None; MAX_RINGS];
        self.particles = [None; MAX_PARTICLES];
        let poses = [
            (143, 108, 18),
            (334, 106, 19),
            (240, 161, 21),
            (107, 211, 18),
            (368, 205, 19),
        ];
        for (slot, (x, y, radius)) in poses.into_iter().enumerate() {
            let color = PALETTE[if self.scene == 1 {
                (slot + 1) % 5
            } else {
                slot
            }];
            self.pads[slot] = Some(Pad {
                id: slot as u32 + 1,
                letter: b'A' + slot as u8,
                pos: Vec2::new(x, y),
                radius: Fixed64::from_int(radius),
                color,
                bounce: Fixed64::from_ratio(11, 10),
                pulse: Fixed64::ZERO,
            });
        }
        self.selected = 0;
        self.next_id = 6;
        self.hits = 0;
        self.sim_time = Fixed64::ZERO;
        self.flash = Fixed64::ZERO;
        self.paused = false;
        self.tilt = Vec2::default();
        self.target = Vec2::default();
        self.keys = 0;
        self.clock.reset();
        self.ring_cursor = 0;
        self.particle_cursor = 0;
        for (x, y, vx, vy) in [
            (74, 83, 82, 25),
            (221, 115, 66, 21),
            (406, 87, -83, 32),
            (48, 196, 83, -60),
            (300, 183, -60, -79),
        ] {
            let _ = self.spawn_at(Vec2::new(x, y), Vec2::new(vx, vy));
        }
        self.page = Page::Play;
        self.inspector = false;
        self.add_mode = false;
        self.notify("SCENE LOADED / EDITS RESET");
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn set_page(&mut self, page: Page) -> ChangeSet {
        self.cancel_input();
        self.page = page;
        self.inspector = false;
        self.add_mode = false;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT
    }

    pub(crate) fn toggle_pause(&mut self) -> ChangeSet {
        if self.page != Page::Play {
            let _ = self.set_page(Page::Play);
        }
        self.paused = !self.paused;
        self.clock.reset();
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn spawn_at(&mut self, pos: Vec2, velocity: Vec2) -> bool {
        let Some(slot) = self.balls.iter().position(Option::is_none) else {
            self.notify("MAXIMUM 8 MARBLES");
            return false;
        };
        self.balls[slot] = Some(Ball {
            pos,
            velocity,
            color: [0xe1f5a6, 0xf0ecce, 0xbadee6][slot % 3],
            trail: [Vec2::default(); TRAIL_LEN],
            trail_len: 0,
            trail_clock: Fixed64::ZERO,
            cooldown: [Fixed64::from_int(-99); MAX_PADS],
        });
        true
    }

    pub(crate) fn drop_ball(&mut self) -> ChangeSet {
        if self.inspector {
            return ChangeSet::NONE;
        }
        if self.page != Page::Play {
            let _ = self.set_page(Page::Play);
        }
        let vx = Fixed64::from_int(65) + Fixed64::from_ratio(i64::from(self.random() % 36), 1);
        let vy = Fixed64::from_int(-15) + Fixed64::from_ratio(i64::from(self.random() % 36), 1);
        if self.spawn_at(Vec2::new(53, 73), Vec2::fixed(vx, vy)) {
            ChangeSet::MODEL | ChangeSet::VISUAL
        } else {
            ChangeSet::VISUAL
        }
    }

    fn pad_at(&self, point: Vec2) -> Option<usize> {
        (0..MAX_PADS).rev().find(|&slot| {
            self.pads[slot]
                .map(|pad| {
                    Vec2::fixed(pad.pos.x - point.x, pad.pos.y - point.y).length()
                        < pad.radius + Fixed64::from_int(5)
                })
                .unwrap_or(false)
        })
    }

    fn bounded(pad: &Pad, position: Vec2) -> Vec2 {
        Vec2::fixed(
            position.x.clamp(
                Fixed64::from_int(18) + pad.radius,
                Fixed64::from_int(462) - pad.radius,
            ),
            position.y.clamp(
                Fixed64::from_int(58) + pad.radius,
                Fixed64::from_int(245) - pad.radius,
            ),
        )
    }

    pub(crate) fn add_pad(&mut self, position: Vec2) -> ChangeSet {
        let Some(slot) = self.pads.iter().position(Option::is_none) else {
            self.notify("MAXIMUM 6 PADS");
            return ChangeSet::VISUAL;
        };
        let letter = (b'A'..=b'F')
            .find(|letter| !self.pads.iter().flatten().any(|pad| pad.letter == *letter))
            .unwrap_or(b'F');
        let mut pad = Pad {
            id: self.next_id,
            letter,
            pos: position,
            radius: Fixed64::from_int(16),
            color: PALETTE[5],
            bounce: Fixed64::from_ratio(11, 10),
            pulse: Fixed64::ZERO,
        };
        pad.pos = Self::bounded(&pad, position);
        if self.pads.iter().flatten().any(|other| {
            Vec2::fixed(other.pos.x - pad.pos.x, other.pos.y - pad.pos.y).length()
                < other.radius + pad.radius + Fixed64::from_int(5)
        }) {
            self.notify("TOO CLOSE / CHOOSE EMPTY SPACE");
            return ChangeSet::VISUAL;
        }
        self.next_id = self.next_id.saturating_add(1);
        self.pads[slot] = Some(pad);
        self.selected = slot;
        self.add_mode = false;
        for ball in self.balls.iter_mut().flatten() {
            ball.cooldown[slot] = Fixed64::from_int(-99);
        }
        self.pulse(slot, false);
        self.notify("PAD ADDED");
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn remove_selected(&mut self) -> ChangeSet {
        if self.pad_count() <= 1 {
            self.notify("KEEP AT LEAST ONE PAD");
            return ChangeSet::VISUAL;
        }
        let slot = self.selected;
        self.pads[slot] = None;
        for ball in self.balls.iter_mut().flatten() {
            ball.cooldown[slot] = Fixed64::from_int(-99);
        }
        self.selected = self
            .pads
            .iter()
            .position(Option::is_some)
            .expect("removing a pad preserves at least one pad");
        self.inspector = false;
        self.notify("PAD REMOVED");
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn toggle_add_mode(&mut self) -> ChangeSet {
        if self.page != Page::Edit {
            let _ = self.set_page(Page::Edit);
        }
        if self.pad_count() >= MAX_PADS {
            self.notify("MAXIMUM 6 PADS");
            return ChangeSet::VISUAL;
        }
        self.add_mode = !self.add_mode;
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn set_inspector(&mut self, open: bool) -> ChangeSet {
        if open && self.page != Page::Edit {
            return ChangeSet::NONE;
        }
        self.inspector = open;
        self.add_mode = false;
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::LAYOUT
    }

    pub(crate) fn set_gravity(&mut self, value: Fixed64) -> ChangeSet {
        let clamped = value.clamp(Fixed64::ZERO, Fixed64::from_ratio(16, 10));
        let step = Fixed64::from_ratio(5, 100);
        let steps = ((clamped + step / 2) / step).to_int();
        self.gravity = Fixed64::from_ratio(steps * 5, 100);
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn set_radius(&mut self, value: Fixed64) -> ChangeSet {
        if let Some(pad) = self.pads[self.selected].as_mut() {
            pad.radius = Fixed64::from_int(
                (value + Fixed64::from_ratio(1, 2))
                    .clamp(Fixed64::from_int(13), Fixed64::from_ratio(235, 10))
                    .to_int(),
            );
            pad.pos = Self::bounded(pad, pad.pos);
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn set_bounce(&mut self, value: Fixed64) -> ChangeSet {
        if let Some(pad) = self.pads[self.selected].as_mut() {
            let clamped = value.clamp(Fixed64::from_ratio(7, 10), Fixed64::from_ratio(14, 10));
            let steps = ((clamped - Fixed64::from_ratio(7, 10) + Fixed64::from_ratio(25, 1_000))
                / Fixed64::from_ratio(5, 100))
            .to_int();
            pad.bounce = Fixed64::from_ratio(70 + steps * 5, 100);
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn set_color(&mut self, index: usize) -> ChangeSet {
        if index < 5 {
            if let Some(pad) = self.pads[self.selected].as_mut() {
                pad.color = PALETTE[index];
            }
            self.pulse(self.selected, false);
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn toggle_trails(&mut self) -> ChangeSet {
        self.trails = !self.trails;
        if !self.trails {
            for ball in self.balls.iter_mut().flatten() {
                ball.trail_len = 0;
            }
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn toggle_feedback(&mut self) -> ChangeSet {
        self.feedback = !self.feedback;
        if !self.feedback {
            self.rings = [None; MAX_RINGS];
            self.particles = [None; MAX_PARTICLES];
        }
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn begin_board_drag(&mut self, point: Vec2) -> ChangeSet {
        if self.inspector
            || !matches!(self.page, Page::Play | Page::Edit)
            || point.y < Fixed64::from_int(52)
            || point.y > Fixed64::from_int(251)
        {
            return ChangeSet::NONE;
        }
        if self.add_mode {
            return self.add_pad(point);
        }
        let slot = self.pad_at(point);
        let original = slot.map(|i| self.pads[i].unwrap().pos).unwrap_or(point);
        self.drag = Some(Drag {
            slot,
            start: point,
            offset: Vec2::fixed(point.x - original.x, point.y - original.y),
            position: slot.map(|_| DragTransaction::begin(original)),
        });
        if let Some(slot) = slot {
            self.selected = slot;
            self.pulse(slot, false);
        }
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn move_board_drag(&mut self, point: Vec2) -> ChangeSet {
        let Some(mut drag) = self.drag else {
            return ChangeSet::NONE;
        };
        if self.page == Page::Edit {
            if let (Some(slot), Some(mut transaction)) = (drag.slot, drag.position) {
                if let Some(pad) = self.pads[slot].as_mut() {
                    let next = Self::bounded(
                        pad,
                        Vec2::fixed(point.x - drag.offset.x, point.y - drag.offset.y),
                    );
                    pad.pos = transaction.update(next);
                    drag.position = Some(transaction);
                }
            }
        } else if self.page == Page::Play && drag.slot.is_none() {
            self.target = Vec2::fixed(
                ((point.x - drag.start.x) / Fixed64::from_int(75))
                    .clamp(-Fixed64::ONE, Fixed64::ONE),
                ((point.y - drag.start.y) / Fixed64::from_int(65))
                    .clamp(-Fixed64::from_ratio(14, 10), Fixed64::ONE),
            );
        }
        self.drag = Some(drag);
        ChangeSet::MODEL | ChangeSet::VISUAL
    }

    pub(crate) fn end_board_drag(&mut self, cancel: bool) -> ChangeSet {
        if let Some(drag) = self.drag.take() {
            if let (Some(slot), Some(transaction)) = (drag.slot, drag.position) {
                if let Some(pad) = self.pads[slot].as_mut() {
                    pad.pos = if cancel {
                        transaction.cancel()
                    } else {
                        transaction.commit()
                    };
                }
            }
        }
        self.target = Vec2::default();
        ChangeSet::MODEL | ChangeSet::VISUAL | ChangeSet::PERSISTENCE
    }

    pub(crate) fn cancel_input(&mut self) {
        let _ = self.end_board_drag(true);
        self.keys = 0;
        self.clock.reset();
    }

    pub(crate) fn pulse(&mut self, slot: usize, count: bool) {
        let Some(mut pad) = self.pads[slot] else {
            return;
        };
        pad.pulse = Fixed64::ONE;
        self.pads[slot] = Some(pad);
        if count {
            self.hits = self.hits.saturating_add(1);
        }
        if !self.feedback {
            return;
        }
        self.rings[self.ring_cursor] = Some(Ring {
            pos: pad.pos,
            radius: pad.radius,
            age: Fixed64::ZERO,
            color: pad.color,
        });
        self.ring_cursor = (self.ring_cursor + 1) % MAX_RINGS;
        const VECTORS: [Vec2; 8] = [
            Vec2::fixed(Fixed64::ONE, Fixed64::ZERO),
            Vec2::fixed(
                Fixed64::from_ratio(707, 1000),
                Fixed64::from_ratio(707, 1000),
            ),
            Vec2::fixed(Fixed64::ZERO, Fixed64::ONE),
            Vec2::fixed(
                Fixed64::from_ratio(-707, 1000),
                Fixed64::from_ratio(707, 1000),
            ),
            Vec2::fixed(Fixed64::from_int(-1), Fixed64::ZERO),
            Vec2::fixed(
                Fixed64::from_ratio(-707, 1000),
                Fixed64::from_ratio(-707, 1000),
            ),
            Vec2::fixed(Fixed64::ZERO, Fixed64::from_int(-1)),
            Vec2::fixed(
                Fixed64::from_ratio(707, 1000),
                Fixed64::from_ratio(-707, 1000),
            ),
        ];
        for _ in 0..4 {
            let vector = VECTORS[(self.random() as usize) % VECTORS.len()];
            let speed = Fixed64::from_int(13 + i64::from(self.random() % 23));
            self.particles[self.particle_cursor] = Some(Particle {
                pos: Vec2::fixed(
                    pad.pos.x + vector.x * pad.radius,
                    pad.pos.y + vector.y * pad.radius,
                ),
                velocity: Vec2::fixed(vector.x * speed, vector.y * speed),
                age: Fixed64::ZERO,
                color: pad.color,
            });
            self.particle_cursor = (self.particle_cursor + 1) % MAX_PARTICLES;
        }
    }

    pub(crate) fn advance_ms(&mut self, elapsed_ms: u16) -> ChangeSet {
        let active = self.running();
        let steps = self.clock.steps(elapsed_ms, active);
        for _ in 0..steps {
            self.physics_step();
        }
        let dt = Fixed64::from_ratio(i64::from(elapsed_ms.min(60)), 1_000);
        let animated = self.needs_animation();
        for pad in self.pads.iter_mut().flatten() {
            pad.pulse = (pad.pulse - dt * Fixed64::from_ratio(34, 10)).max(Fixed64::ZERO);
        }
        for ring in &mut self.rings {
            if let Some(value) = ring {
                value.age += dt;
                value.radius += dt * Fixed64::from_int(23);
                if value.age >= Fixed64::from_ratio(45, 100) {
                    *ring = None;
                }
            }
        }
        for particle in &mut self.particles {
            if let Some(value) = particle {
                value.pos.x += value.velocity.x * dt;
                value.pos.y += value.velocity.y * dt;
                value.age += dt;
                if value.age >= Fixed64::from_ratio(43, 100) {
                    *particle = None;
                }
            }
        }
        self.flash = (self.flash - dt * Fixed64::from_int(2)).max(Fixed64::ZERO);
        self.toast_left = (self.toast_left - dt).max(Fixed64::ZERO);
        if self.toast_left.is_zero() {
            self.toast = "";
        }
        if animated {
            ChangeSet::VISUAL
        } else {
            ChangeSet::NONE
        }
    }

    fn physics_step(&mut self) {
        self.sim_time += STEP;
        let mut target = self.target;
        if self.keys & 1 != 0 {
            target.x = -Fixed64::ONE;
        }
        if self.keys & 2 != 0 {
            target.x = Fixed64::ONE;
        }
        if self.keys & 4 != 0 {
            target.y = -Fixed64::from_ratio(14, 10);
        }
        if self.keys & 8 != 0 {
            target.y = Fixed64::ONE;
        }
        let response = (STEP * Fixed64::from_int(14)).min(Fixed64::ONE);
        self.tilt.x += (target.x - self.tilt.x) * response;
        self.tilt.y += (target.y - self.tilt.y) * response;
        let gx = self.tilt.x * Fixed64::from_int(190);
        let gy = self.gravity * Fixed64::from_int(150) + self.tilt.y * Fixed64::from_int(200);
        let drag = Fixed64::ONE - Fixed64::from_ratio(18, 1_000) * STEP;
        let mut collisions = [0_u8; MAX_PADS];
        for ball in self.balls.iter_mut().flatten() {
            ball.velocity.x = (ball.velocity.x + gx * STEP) * drag;
            ball.velocity.y = (ball.velocity.y + gy * STEP) * drag;
            let speed = ball.velocity.length();
            if speed > Fixed64::from_int(240) {
                let scale = Fixed64::from_int(240) / speed;
                ball.velocity.x = ball.velocity.x * scale;
                ball.velocity.y = ball.velocity.y * scale;
            }
            ball.pos.x += ball.velocity.x * STEP;
            ball.pos.y += ball.velocity.y * STEP;
            Self::collide_walls(ball, self.gravity, &mut self.flash);
            for rail in RAILS {
                Self::collide_rail(ball, rail);
            }
            for (slot, pad) in self.pads.iter().enumerate() {
                let Some(pad) = pad else { continue };
                let mut dx = ball.pos.x - pad.pos.x;
                let mut dy = ball.pos.y - pad.pos.y;
                let mut distance = Vec2::fixed(dx, dy).length();
                let minimum = pad.radius + Fixed64::from_int(4);
                if distance < minimum {
                    if distance < Fixed64::from_ratio(1, 10_000) {
                        dx = Fixed64::ONE;
                        dy = Fixed64::ZERO;
                        distance = Fixed64::ONE;
                    }
                    let nx = dx / distance;
                    let ny = dy / distance;
                    ball.pos = Vec2::fixed(
                        pad.pos.x + nx * (minimum + Fixed64::from_ratio(8, 100)),
                        pad.pos.y + ny * (minimum + Fixed64::from_ratio(8, 100)),
                    );
                    let normal_speed = ball.velocity.x * nx + ball.velocity.y * ny;
                    if normal_speed.is_negative() {
                        let impulse = (Fixed64::ONE + pad.bounce) * normal_speed;
                        ball.velocity.x -= impulse * nx;
                        ball.velocity.y -= impulse * ny;
                        ball.velocity.x += nx * Fixed64::from_int(21);
                        ball.velocity.y += ny * Fixed64::from_int(21);
                        if self.sim_time - ball.cooldown[slot] > Fixed64::from_ratio(13, 100) {
                            ball.cooldown[slot] = self.sim_time;
                            collisions[slot] = collisions[slot].saturating_add(1);
                        }
                    }
                }
            }
            ball.trail_clock += STEP;
            if ball.trail_clock >= Fixed64::from_ratio(1, 30) {
                ball.trail_clock -= Fixed64::from_ratio(1, 30);
                if self.trails {
                    if ball.trail_len == TRAIL_LEN {
                        ball.trail.copy_within(1..TRAIL_LEN, 0);
                        ball.trail_len -= 1;
                    }
                    ball.trail[ball.trail_len] = ball.pos;
                    ball.trail_len += 1;
                } else {
                    ball.trail_len = 0;
                }
            }
        }
        self.collide_balls();
        for ball in self.balls.iter_mut().flatten() {
            ball.pos.x = ball
                .pos
                .x
                .clamp(Fixed64::from_int(19), Fixed64::from_int(461));
            ball.pos.y = ball
                .pos
                .y
                .clamp(Fixed64::from_int(59), Fixed64::from_int(244));
        }
        for (slot, count) in collisions.into_iter().enumerate() {
            for _ in 0..count {
                self.pulse(slot, true);
            }
        }
    }

    fn collide_walls(ball: &mut Ball, gravity: Fixed64, flash: &mut Fixed64) {
        let restitution = Fixed64::from_ratio(96, 100);
        if ball.pos.x < Fixed64::from_int(19) {
            ball.pos.x = Fixed64::from_int(19);
            ball.velocity.x = ball.velocity.x.abs() * restitution;
        }
        if ball.pos.x > Fixed64::from_int(461) {
            ball.pos.x = Fixed64::from_int(461);
            ball.velocity.x = -ball.velocity.x.abs() * restitution;
        }
        if ball.pos.y < Fixed64::from_int(59) {
            ball.pos.y = Fixed64::from_int(59);
            ball.velocity.y = ball.velocity.y.abs() * restitution;
        }
        if ball.pos.y > Fixed64::from_int(244) {
            ball.pos.y = Fixed64::from_int(244);
            let boost = Fixed64::from_int(115) + gravity * Fixed64::from_int(26);
            ball.velocity.y = -(ball.velocity.y.abs() * restitution).max(boost);
            ball.velocity.x += (Fixed64::from_int(240) - ball.pos.x) * Fixed64::from_ratio(17, 100);
            *flash = Fixed64::from_ratio(6, 10);
        }
    }

    fn collide_rail(ball: &mut Ball, rail: (i64, i64, i64, i64)) {
        let (ax, ay, bx, by) = rail;
        let a = Vec2::new(ax, ay);
        let delta = Vec2::new(bx - ax, by - ay);
        let denominator = delta.x * delta.x + delta.y * delta.y;
        let t = ((ball.pos.x - a.x) * delta.x + (ball.pos.y - a.y) * delta.y) / denominator;
        let t = t.clamp(Fixed64::ZERO, Fixed64::ONE);
        let closest = Vec2::fixed(a.x + delta.x * t, a.y + delta.y * t);
        let mut normal = Vec2::fixed(ball.pos.x - closest.x, ball.pos.y - closest.y);
        let mut distance = normal.length();
        if distance >= Fixed64::from_ratio(63, 10) {
            return;
        }
        if distance < Fixed64::from_ratio(1, 10_000) {
            normal = Vec2::fixed(-delta.y, delta.x);
            distance = normal.length();
        }
        normal.x = normal.x / distance;
        normal.y = normal.y / distance;
        ball.pos = Vec2::fixed(
            closest.x + normal.x * Fixed64::from_ratio(635, 100),
            closest.y + normal.y * Fixed64::from_ratio(635, 100),
        );
        let speed = ball.velocity.x * normal.x + ball.velocity.y * normal.y;
        if speed.is_negative() {
            let impulse = Fixed64::from_ratio(19, 10) * speed;
            ball.velocity.x -= impulse * normal.x;
            ball.velocity.y -= impulse * normal.y;
        }
    }

    fn collide_balls(&mut self) {
        for left_slot in 0..MAX_BALLS {
            for right_slot in (left_slot + 1)..MAX_BALLS {
                let (left, right) = self.balls.split_at_mut(right_slot);
                let (Some(a), Some(b)) = (&mut left[left_slot], &mut right[0]) else {
                    continue;
                };
                let mut delta = Vec2::fixed(b.pos.x - a.pos.x, b.pos.y - a.pos.y);
                let mut distance = delta.length();
                if distance >= Fixed64::from_int(8) {
                    continue;
                }
                if distance < Fixed64::from_ratio(1, 10_000) {
                    delta = Vec2::fixed(Fixed64::ONE, Fixed64::ZERO);
                    distance = Fixed64::ONE;
                }
                let normal = Vec2::fixed(delta.x / distance, delta.y / distance);
                let overlap = (Fixed64::from_int(8) - distance) / Fixed64::from_int(2);
                a.pos.x -= normal.x * overlap;
                a.pos.y -= normal.y * overlap;
                b.pos.x += normal.x * overlap;
                b.pos.y += normal.y * overlap;
                let relative = (b.velocity.x - a.velocity.x) * normal.x
                    + (b.velocity.y - a.velocity.y) * normal.y;
                if relative.is_negative() {
                    a.velocity.x += relative * normal.x;
                    a.velocity.y += relative * normal.y;
                    b.velocity.x -= relative * normal.x;
                    b.velocity.y -= relative * normal.y;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_scene_respects_fixed_capacities() {
        let model = MarbleModel::new();
        assert_eq!(model.ball_count(), 5);
        assert_eq!(model.pad_count(), 5);
        assert_eq!(model.page, Page::Play);
    }

    #[test]
    fn ball_and_feedback_storage_stay_bounded() {
        let mut model = MarbleModel::new();
        for _ in 0..20 {
            let _ = model.drop_ball();
        }
        for _ in 0..100 {
            model.pulse(0, true);
        }
        assert_eq!(model.ball_count(), MAX_BALLS);
        assert_eq!(model.rings.iter().flatten().count(), MAX_RINGS);
        assert_eq!(model.particles.iter().flatten().count(), MAX_PARTICLES);
    }

    #[test]
    fn pausing_and_editing_freeze_physics() {
        let mut model = MarbleModel::new();
        model.paused = true;
        let paused = model.balls[0].unwrap().pos;
        for _ in 0..60 {
            let _ = model.advance_ms(16);
        }
        assert_eq!(model.balls[0].unwrap().pos, paused);
        model.paused = false;
        let _ = model.set_page(Page::Edit);
        let time = model.sim_time;
        let _ = model.advance_ms(60);
        assert_eq!(model.sim_time, time);
    }

    #[test]
    fn catch_up_never_exceeds_eight_steps() {
        let mut model = MarbleModel::new();
        let _ = model.advance_ms(u16::MAX);
        assert!(model.sim_time <= STEP * 8);
    }

    #[test]
    fn drag_cancel_restores_pad_and_commit_keeps_it() {
        let mut model = MarbleModel::new();
        let _ = model.set_page(Page::Edit);
        let original = model.selected_pad().pos;
        let _ = model.begin_board_drag(original);
        let _ = model.move_board_drag(Vec2::fixed(original.x + Fixed64::from_int(40), original.y));
        let _ = model.end_board_drag(true);
        assert_eq!(model.selected_pad().pos, original);

        let _ = model.begin_board_drag(original);
        let _ = model.move_board_drag(Vec2::fixed(original.x + Fixed64::from_int(20), original.y));
        let _ = model.end_board_drag(false);
        assert_eq!(
            model.selected_pad().pos.x,
            original.x + Fixed64::from_int(20)
        );
    }

    #[test]
    fn deleting_and_adding_preserves_unique_ids() {
        let mut model = MarbleModel::new();
        for _ in 0..4 {
            let _ = model.remove_selected();
        }
        assert_eq!(model.pad_count(), 1);
        let _ = model.remove_selected();
        assert_eq!(model.pad_count(), 1);
        let _ = model.add_pad(Vec2::new(143, 108));
        let mut ids = [0_u32; MAX_PADS];
        let mut len = 0;
        for pad in model.pads.iter().flatten() {
            assert!(!ids[..len].contains(&pad.id));
            ids[len] = pad.id;
            len += 1;
        }
    }

    #[test]
    fn long_run_stays_inside_world_bounds() {
        let mut model = MarbleModel::new();
        for _ in 0..7_200 {
            let _ = model.advance_ms(8);
        }
        for ball in model.balls.iter().flatten() {
            assert!((Fixed64::from_int(19)..=Fixed64::from_int(461)).contains(&ball.pos.x));
            assert!((Fixed64::from_int(59)..=Fixed64::from_int(244)).contains(&ball.pos.y));
            assert!(ball.trail_len <= TRAIL_LEN);
        }
    }

    #[test]
    fn property_controls_enforce_documented_ranges() {
        let mut model = MarbleModel::new();
        let _ = model.set_gravity(Fixed64::from_int(9));
        let _ = model.set_radius(Fixed64::from_int(99));
        let _ = model.set_bounce(Fixed64::ZERO);
        assert_eq!(model.gravity, Fixed64::from_ratio(16, 10));
        assert_eq!(model.selected_pad().radius, Fixed64::from_int(23));
        assert_eq!(model.selected_pad().bounce, Fixed64::from_ratio(7, 10));
    }

    #[test]
    fn disabling_feedback_releases_all_transient_slots() {
        let mut model = MarbleModel::new();
        model.pulse(0, true);
        assert!(model.rings.iter().any(Option::is_some));
        let _ = model.toggle_feedback();
        assert!(model.rings.iter().all(Option::is_none));
        assert!(model.particles.iter().all(Option::is_none));
        model.pulse(0, true);
        assert!(model.rings.iter().all(Option::is_none));
    }

    #[test]
    fn change_sets_distinguish_layout_and_visual_updates() {
        let mut model = MarbleModel::new();
        let page = model.set_page(Page::Settings);
        assert!(page.contains(ChangeSet::LAYOUT));
        let tick = model.advance_ms(16);
        assert!(!tick.contains(ChangeSet::LAYOUT));
    }
}
