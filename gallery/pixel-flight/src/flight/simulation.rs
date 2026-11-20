use super::{
    Flight, Projection, SMOKE_CAPACITY, Smoke, TRAIL_CAPACITY, TrailPoint, aircraft_point, follow,
    wrap_angle,
};
use crate::{
    jet,
    math::{FixedMath as _, PI},
    world::{self, Ground},
};
use mirui::types::{Fixed, Fixed64};

#[derive(Clone, Copy)]
struct FlightLimits {
    clearance: Fixed,
    ceiling: Fixed,
    climb_rate: Fixed,
    turn_rate: Fixed,
}

impl Flight {
    pub(crate) fn step_above(
        &mut self,
        world: Ground<'_>,
        milliseconds: u16,
        width: usize,
        height: usize,
    ) {
        self.elapsed_ms += u64::from(milliseconds.min(50));
        let now = (Fixed64::from_int(self.elapsed_ms as i64) / 1000).to_fixed();
        let dt = now - self.time;
        if dt == Fixed::ZERO {
            return;
        }
        let previous_camera = self.projection(width, height);
        self.time = now;
        let ready = self.update_thrust(dt);
        let clearance = self
            .settings
            .clearance
            .clamp(Fixed::from_int(2), Fixed::from_int(60));
        self.update_terrain_floor(world, clearance, dt);
        let ceiling = self.floor.max(self.settings.altitude).max(
            self.settings
                .ceiling
                .clamp(Fixed::from_int(20), Fixed::from_int(900)),
        );
        let limits = FlightLimits {
            clearance,
            ceiling,
            climb_rate: self.speed * Fixed::from_ratio(55, 100),
            turn_rate: self.settings.agility.max(Fixed::from_ratio(1, 10)),
        };
        let (turn, pilot_override) =
            self.steering(ready, &previous_camera, [width, height], limits, dt);
        let heading = self.move_player(world, dt, turn, pilot_override, limits);
        self.move_formation(world, heading, clearance, dt);
        self.record_trajectories(limits.turn_rate, dt);
        self.emit_smoke(dt);
        self.follow_camera(world, &previous_camera, heading, dt);
    }

    fn update_thrust(&mut self, dt: Fixed) -> bool {
        let s = self.settings;
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
        ready
    }

    fn update_terrain_floor(&mut self, world: Ground<'_>, clearance: Fixed, dt: Fixed) {
        let j = &self.jets[0];
        let (sin, cos) = j.yaw.sin_cos();
        let mut ground = Fixed::MIN;
        for i in 0..=4 {
            let distance = self.speed * Fixed::from_ratio(16, 100) * Fixed::from_int(i);
            for side in -1..=1 {
                ground = ground.max(
                    world.height(
                        (j.position[0]
                            + sin * distance
                            + cos * Fixed::from_int(side) * Fixed::from_int(7))
                        .trunc_to_int(),
                        (j.position[1] - cos * distance
                            + sin * Fixed::from_int(side) * Fixed::from_int(7))
                        .trunc_to_int(),
                    ),
                );
            }
        }
        let floor = ground + clearance;
        if self.floor == Fixed::ZERO {
            self.floor = floor;
        }
        let floor_rate = if floor > self.floor {
            Fixed::from_int(9)
        } else {
            Fixed::from_int(3)
        };
        follow(&mut self.floor, floor, floor_rate, dt);
    }

    fn steering(
        &mut self,
        ready: bool,
        old_camera: &Projection,
        [width, height]: [usize; 2],
        limits: FlightLimits,
        dt: Fixed,
    ) -> (Fixed, bool) {
        if !ready {
            return (Fixed::ZERO, false);
        }
        let rate = limits.turn_rate;
        if self.controls.keyboard {
            let bits = self.controls.keys;
            self.altitude_goal +=
                Fixed::from_int(i32::from(bits & 48 != 0) - i32::from(bits & 192 != 0))
                    * limits.climb_rate
                    * dt;
            let turn = Fixed::from_int(i32::from(bits & 12 != 0) - i32::from(bits & 3 != 0)) * rate;
            return (turn, true);
        }
        let Some(pointer) = self
            .controls
            .pointer
            .filter(|_| self.controls.pointer_idle < Fixed::from_int(5))
        else {
            return (self.autopilot_turn(rate, dt), false);
        };
        let following =
            self.chase[2].to_fixed() + self.cockpit[2].to_fixed() > Fixed::from_ratio(5, 10);
        if following {
            let turn = ((pointer[0] - old_camera.camera.center[0])
                / (Fixed::from_int(width as i32) * Fixed::from_ratio(26, 100)))
            .clamp(-Fixed::ONE, Fixed::ONE);
            let center_y = old_camera.steer_y;
            let raw = if pointer[1] < center_y {
                (center_y - pointer[1]) / center_y.max(Fixed::ONE)
            } else {
                -(pointer[1] - center_y)
                    / (Fixed::from_int(height as i32) - center_y).max(Fixed::ONE)
            };
            let up = raw.signum() * (raw.abs() - Fixed::from_ratio(8, 100)).max(Fixed::ZERO)
                / Fixed::from_ratio(92, 100);
            let cruise = self.settings.altitude;
            self.altitude_goal = if up >= Fixed::ZERO {
                cruise + up * (limits.ceiling - cruise)
            } else {
                cruise + up * (cruise - self.floor)
            };
            let turn = if turn.abs() > Fixed::from_ratio(6, 100) {
                turn * rate
            } else {
                Fixed::ZERO
            };
            return (turn, true);
        }
        let player = &self.jets[0];
        let (sin, cos) = old_camera.yaw.sin_cos();
        let Some(point) = old_camera.project_with_yaw(player.position, (sin, cos)) else {
            return (Fixed::ZERO, false);
        };
        let dx = pointer[0] - point[0];
        let dy = pointer[1] - point[1];
        if dx * dx + dy * dy <= Fixed::from_int(324) {
            return (Fixed::ZERO, true);
        }
        let heading = (dx * cos + dy * sin).atan2(-(-dx * sin + dy * cos));
        let turn =
            (wrap_angle(heading - player.yaw) * Fixed::from_ratio(26, 10)).clamp(-rate, rate);
        (turn, true)
    }

    fn autopilot_turn(&mut self, rate: Fixed, dt: Fixed) -> Fixed {
        let time = self.time;
        let turn = rate
            * (Fixed::from_ratio(62, 100)
                * (time * Fixed::from_ratio(31, 100) + Fixed::from_ratio(13, 10)).sin()
                + Fixed::from_ratio(38, 100)
                    * (time * Fixed::from_ratio(73, 100) + Fixed::from_ratio(4, 10)).sin()
                + Fixed::from_ratio(22, 100) * (time * Fixed::from_ratio(17, 100)).sin());
        if self.settings.stunts {
            self.auto_stunt -= dt;
            if self.auto_stunt <= Fixed::ZERO {
                self.roll();
                let seed = (time * Fixed::from_int(10)).trunc_to_int() as u32;
                self.auto_stunt = Fixed::from_int(9)
                    + Fixed::from_int(i32::from(world::hash8(seed))) / Fixed::from_int(255)
                        * Fixed::from_int(7);
            }
        }
        turn.clamp(-rate, rate)
    }

    fn move_player(
        &mut self,
        world: Ground<'_>,
        dt: Fixed,
        want: Fixed,
        flown: bool,
        limits: FlightLimits,
    ) -> (Fixed, Fixed) {
        let FlightLimits {
            clearance,
            ceiling,
            climb_rate: climb_max,
            turn_rate: rate,
        } = limits;
        let s = self.settings;
        let j = &mut self.jets[0];
        follow(&mut j.yaw_rate, want, Fixed::from_ratio(26, 10), dt);
        j.yaw += j.yaw_rate * dt;
        let turn = j.yaw_rate / rate;
        let bank = turn.clamp(-Fixed::ONE, Fixed::ONE);
        follow(
            &mut j.roll,
            bank * s.bank * PI / Fixed::from_int(180),
            Fixed::from_int(5),
            dt,
        );
        let (sin, cos) = j.yaw.sin_cos();
        j.velocity = [sin * self.speed, -cos * self.speed];
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
        let hard = world.height(j.position[0].trunc_to_int(), j.position[1].trunc_to_int())
            + clearance * Fixed::from_ratio(7, 10);
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
            turn.abs() * Fixed::from_ratio(1, 10) + self.climb_tilt,
            Fixed::from_int(4),
            dt,
        );
        let goals = [
            bank * Fixed::from_ratio(64, 10)
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
        (sin, cos)
    }

    fn move_formation(
        &mut self,
        world: Ground<'_>,
        (sin, cos): (Fixed, Fixed),
        clearance: Fixed,
        dt: Fixed,
    ) {
        let [player, wingmen @ ..] = &mut self.jets[..self.count] else {
            unreachable!("flight has a player")
        };
        let position = player.position;
        let s = self.settings;
        for (i, j) in (1..).zip(wingmen) {
            let side = if i == 1 { -Fixed::ONE } else { Fixed::ONE };
            let x = side
                * (Fixed::from_int((jet::WIDTH) as i32) * Fixed::from_ratio(12, 10) * s.jet_scale
                    + Fixed::from_int(2));
            let y =
                Fixed::from_int((jet::LENGTH) as i32) * Fixed::from_ratio(74, 100) * s.jet_scale;
            let target = [
                position[0] + cos * x - sin * y,
                position[1] + sin * x + cos * y,
            ];
            for (k, target) in target.into_iter().enumerate() {
                let old = j.position[k];
                j.position[k] += player.velocity[k] * dt;
                follow(&mut j.position[k], target, Fixed::from_ratio(32, 10), dt);
                j.velocity[k] = (j.position[k] - old) / dt;
            }
            j.yaw +=
                wrap_angle(player.yaw - j.yaw) * (Fixed::ONE - (-dt * Fixed::from_int(4)).exp());
            follow(&mut j.roll, player.roll, Fixed::from_ratio(32, 10), dt);
            j.pitch = player.pitch;
            follow(
                &mut j.position[2],
                position[2]
                    + (self.time * Fixed::from_ratio(11, 10)
                        + Fixed::from_int(i) * Fixed::from_int(2))
                    .sin()
                        * Fixed::from_ratio(12, 10),
                Fixed::from_int(3),
                dt,
            );
            j.position[2] = j.position[2].max(
                world.height(j.position[0].trunc_to_int(), j.position[1].trunc_to_int())
                    + clearance * Fixed::from_ratio(7, 10),
            );
        }
    }

    fn record_trajectories(&mut self, rate: Fixed, dt: Fixed) {
        let s = self.settings;
        let turn_load = self.jets[0].yaw_rate.abs() / rate;
        for j in &mut self.jets[..self.count] {
            let load = (turn_load
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
                let axes = j.axes();
                for w in 0..2 {
                    j.trails[w][j.trail_head] = TrailPoint {
                        position: aircraft_point(
                            j.position,
                            axes,
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
    }

    fn emit_smoke(&mut self, dt: Fixed) {
        let s = self.settings;
        let velocity = self.jets[0].velocity;
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
                        + Fixed::from_int((world::hash8((head * 31 + 7) as u32)) as i32)
                            / Fixed::from_int(255)
                            * Fixed::from_ratio(12, 10))
                        * s.jet_scale,
                    push: [
                        -velocity[0] * Fixed::from_ratio(3, 10)
                            + (Fixed::from_int((world::hash8((head * 17 + 3) as u32)) as i32)
                                - Fixed::from_int(128))
                                * Fixed::from_ratio(5, 100),
                        -velocity[1] * Fixed::from_ratio(3, 10)
                            + (Fixed::from_int((world::hash8((head * 13 + 5) as u32)) as i32)
                                - Fixed::from_int(128))
                                * Fixed::from_ratio(5, 100),
                    ],
                    strength: (Fixed::from_ratio(46, 100)
                        + Fixed::from_ratio(36, 100) * self.boost)
                        * (s.afterburner * Fixed::from_ratio(14, 10))
                            .clamp(Fixed::ZERO, Fixed::ONE),
                    life: Fixed::from_ratio(75, 100)
                        + Fixed::from_ratio(5, 10) * self.boost
                        + Fixed::from_int((world::hash8((head * 5) as u32)) as i32)
                            / Fixed::from_int(255)
                            * Fixed::from_ratio(3, 10),
                };
                self.smoke_head = (head + 1) % SMOKE_CAPACITY;
            }
        }
    }
}
