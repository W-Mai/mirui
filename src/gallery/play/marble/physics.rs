use super::model::MarbleModel;
use super::types::{
    Ball, MAX_BALLS, MAX_PADS, MAX_PARTICLES, MAX_RINGS, Particle, Ring, TRAIL_LEN, Vec2,
};
use crate::types::Fixed64;

pub(super) const STEP: Fixed64 = Fixed64::from_ratio(1, 120);

const RAILS: [(i64, i64, i64, i64); 4] = [
    (36, 145, 78, 165),
    (215, 80, 264, 70),
    (269, 213, 308, 234),
    (404, 144, 437, 122),
];

impl MarbleModel {
    pub(super) fn random(&mut self) -> u32 {
        let mut value = self.rng;
        value ^= value << 13;
        value ^= value >> 17;
        value ^= value << 5;
        self.rng = value;
        value
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

    pub(crate) fn pulse(&mut self, slot: usize, count: bool) {
        self.pulse_with_gain(slot, count, 165);
    }

    fn pulse_with_gain(&mut self, slot: usize, count: bool, gain: u8) {
        let Some(mut pad) = self.pads[slot] else {
            return;
        };
        if count {
            if self.sim_time - pad.last_hit <= Fixed64::from_ratio(105, 1_000) {
                return;
            }
            pad.last_hit = self.sim_time;
        }
        pad.pulse = Fixed64::ONE;
        self.pads[slot] = Some(pad);
        if count {
            self.hits = self.hits.saturating_add(1);
            self.emit_pad_sound(slot, gain, false);
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

    pub(super) fn physics_step(&mut self) {
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
        let mut collision_gain = [0_u8; MAX_PADS];
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
                let minimum = pad.radius + Fixed64::from_int(4);
                let Some(mut distance) = Vec2::fixed(dx, dy).length_below(minimum) else {
                    continue;
                };
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
                        let impact = normal_speed.abs().to_int();
                        collision_gain[slot] =
                            collision_gain[slot].max((110 + impact * 3 / 5).clamp(120, 255) as u8);
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
                self.pulse_with_gain(slot, true, collision_gain[slot]);
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
        let Some(mut distance) = normal.length_below(Fixed64::from_ratio(63, 10)) else {
            return;
        };
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
                let Some(mut distance) = delta.length_below(Fixed64::from_int(8)) else {
                    continue;
                };
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
