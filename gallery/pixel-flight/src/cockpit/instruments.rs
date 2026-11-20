use super::*;

impl Cockpit {
    pub(super) fn paint_indicators(&self, live: &mut Live<'_, '_>, flight: &Flight) {
        let b = &self.inks[1..=9];
        let a = &self.inks[17..=22];
        for (i, lamp) in self.lamps.iter().enumerate() {
            if flight.boost > Fixed::from_ratio(25, 100)
                || i < 2
                || (i == 2 && (flight.time * Fixed::from_int(2)).trunc_to_int() % 2 == 0)
                || (i == 4
                    && hash8((flight.time * Fixed::from_int(3)).trunc_to_int() as u32 * 7) > 200)
            {
                live.rect(lamp.x, lamp.y, lamp.x + lamp.w, lamp.y + lamp.h, a[4]);
                live.rect(lamp.x, lamp.y, lamp.x + lamp.w, lamp.y + 1, a[5]);
            }
        }
        for (i, key) in self.panel_keys.iter().enumerate() {
            if hash8(
                (i * 53) as u32
                    + (flight.time
                        * (Fixed::from_ratio(7, 10)
                            + Fixed::from_int((i) as i32) * Fixed::from_ratio(23, 100)))
                    .trunc_to_int() as u32,
            ) <= 150
            {
                live.rect(key.x, key.y, key.x + key.w, key.y + key.h, b[3]);
            }
        }
    }

    pub(super) fn paint_radar(
        &self,
        live: &mut Live<'_, '_>,
        world: Ground<'_>,
        flight: &Flight,
        prepared: &PreparedCockpit,
    ) {
        let clip = live.clip();
        let j = &flight.jets[0];
        let s = self.scale;
        let radar = self.screens[0];
        let zoom = Fixed::from_int(9) / s;
        let sweep = (flight.time * Fixed::from_ratio(16, 10)).rem_euclid(TAU);
        for yy in 0.max(clip[1] - radar.y)..radar.h.min(clip[3] - radar.y) {
            let dy = yy - radar.h / 2;
            let ahead = Fixed::from_int(-dy) * zoom;
            for xx in 0.max(clip[0] - radar.x)..radar.w.min(clip[2] - radar.x) {
                let dx = xx - radar.w / 2;
                let cell = &self.radar[(yy * radar.w + xx) as usize];
                let right = Fixed::from_int(dx) * zoom;
                let mat = world.material(
                    (j.position[0] + right * prepared.heading[0] + ahead * prepared.heading[1])
                        .trunc_to_int(),
                    (j.position[1] + right * prepared.heading[1] - ahead * prepared.heading[0])
                        .trunc_to_int(),
                );
                let mut tone = if mat >= 12 {
                    4
                } else if mat >= 4 {
                    3
                } else if mat >= 2 || yy & 1 != 0 {
                    1
                } else {
                    0
                };
                if mat < 4 && (dx == 0 || dy == 0) {
                    tone = 2;
                }
                if mat < 12 && cell.flags & RadarCell::RING != 0 {
                    tone = tone.max(if mat >= 4 { 4 } else { 2 });
                }
                if cell.flags & RadarCell::INSIDE != 0 {
                    let angle = (sweep - cell.bearing).rem_euclid(TAU);
                    if angle < Fixed::from_ratio(12, 100) {
                        tone = 5;
                    } else if angle < Fixed::from_ratio(7, 10) && tone < 5 {
                        tone += 1;
                    }
                }
                live.put(radar.x + xx, radar.y + yy, GREEN[tone]);
            }
        }
        for other in &flight.jets[1..flight.count] {
            let ex = other.position[0] - j.position[0];
            let ey = other.position[1] - j.position[1];
            let x = radar.x
                + radar.w / 2
                + (((ex * prepared.heading[0] + ey * prepared.heading[1]) / zoom).round())
                    .trunc_to_int();
            let y = radar.y + radar.h / 2
                - (((ex * prepared.heading[1] - ey * prepared.heading[0]) / zoom).round())
                    .trunc_to_int();
            if x > radar.x + 1
                && x < radar.x + radar.w - 2
                && y > radar.y + 1
                && y < radar.y + radar.h - 2
            {
                live.rect(x, y, x + 2, y + 2, GREEN[5]);
            }
        }
        for (dx, dy) in [(0, -2), (0, -1), (-1, 0), (0, 0), (1, 0), (-2, 1), (2, 1)] {
            live.put(
                radar.x + radar.w / 2 + dx,
                radar.y + radar.h / 2 + dy,
                GREEN[5],
            );
        }
        for i in 0..3 {
            live.rect(
                radar.x + 3,
                radar.y + 3 + i * 3,
                radar.x + 3 + if i == 1 { 9 } else { 13 },
                radar.y + 5 + i * 3,
                GREEN[3],
            );
            live.rect(
                radar.x + radar.w - 15,
                radar.y + radar.h - 11 + i * 3,
                radar.x + radar.w - 3,
                radar.y + radar.h - 9 + i * 3,
                GREEN[3],
            );
        }
        instrument_glass(live, radar);
    }

    pub(super) fn paint_status(&self, live: &mut Live<'_, '_>, flight: &Flight) {
        let clip = live.clip();
        let r = |v| scaled_pixel(v, self.scale);
        let a = &self.inks[17..=22];
        let j = &flight.jets[0];
        let status = self.screens[1];
        for y in 0.max(clip[1] - status.y)..status.h.min(clip[3] - status.y) {
            live.rect(
                status.x,
                status.y + y,
                status.x + status.w,
                status.y + y + 1,
                GREEN[(y & 1) as usize],
            );
        }
        let fit = (Fixed::from_int(status.h - 8) / Fixed::from_int((jet::LENGTH) as i32))
            .min(
                Fixed::from_int(status.w) * Fixed::from_ratio(5, 10)
                    / Fixed::from_int((jet::WIDTH) as i32),
            )
            .max(Fixed::from_ratio(1, 100));
        let jw = ((Fixed::from_int((jet::WIDTH) as i32) * fit).round()).trunc_to_int();
        let jh = ((Fixed::from_int((jet::LENGTH) as i32) * fit).round()).trunc_to_int();
        let jx = status.x
            + ((Fixed::from_int(status.w) * Fixed::from_ratio(31, 100)).round()).trunc_to_int()
            - jw / 2;
        let jy = status.y + (status.h - jh) / 2;
        let at = |xx: i32, yy: i32| {
            if xx < 0 || yy < 0 || xx >= jw || yy >= jh {
                0
            } else {
                jet::PROJECTIONS.top[((Fixed::from_int(yy) / fit).trunc_to_int() as usize)
                    .min(jet::LENGTH - 1)
                    * jet::WIDTH
                    + ((Fixed::from_int(xx) / fit).trunc_to_int() as usize).min(jet::WIDTH - 1)]
            }
        };
        for yy in 0.max(clip[1] - jy)..jh.min(clip[3] - jy) {
            for xx in 0.max(clip[0] - jx)..jw.min(clip[2] - jx) {
                let c = at(xx, yy);
                if c == 0 {
                    continue;
                }
                let edge = at(xx - 1, yy) == 0
                    || at(xx + 1, yy) == 0
                    || at(xx, yy - 1) == 0
                    || at(xx, yy + 1) == 0;
                live.put(
                    jx + xx,
                    jy + yy,
                    if c == 2 {
                        if edge { a[2] } else { a[3] }
                    } else if c == 3 {
                        GREEN[5]
                    } else if edge {
                        GREEN[2]
                    } else {
                        GREEN[3]
                    },
                );
            }
        }
        if hash8((flight.time * Fixed::from_int(12)).trunc_to_int() as u32) > 60 {
            for side in [-1, 1] {
                let xx = jx + jw / 2 + side * 2;
                live.rect(
                    xx - 1,
                    jy + jh,
                    xx + 1,
                    jy + jh + 1 + ((flight.boost * Fixed::from_int(3)).round()).trunc_to_int(),
                    a[5],
                );
            }
        }
        let bh = status.h - 16;
        for engine in 0..2 {
            let bx = status.x
                + ((Fixed::from_int(status.w) * Fixed::from_ratio(66, 100)).round()).trunc_to_int()
                + engine * r(12).max(r(8).max(1));
            let by = status.y + 7;
            let bw = r(8).max(r(5).max(1));
            live.rect(bx - 1, by - 1, bx + bw + 1, by + bh + 1, GREEN[3]);
            live.rect(bx, by, bx + bw, by + bh, GREEN[0]);
            let level = (Fixed::from_ratio(5, 10)
                + Fixed::from_ratio(45, 100) * flight.boost
                + (Fixed::from_int(
                    (hash8(
                        (flight.time * Fixed::from_int(9)).trunc_to_int() as u32
                            + engine as u32 * 31,
                    )) as i32,
                ) - Fixed::from_int(128))
                    / Fixed::from_int(2600))
            .clamp(Fixed::ZERO, Fixed::ONE);
            for yy in 0..((Fixed::from_int(bh) * level).round()).trunc_to_int() {
                if yy % 3 != 2 {
                    live.rect(
                        bx + 1,
                        by + bh - 1 - yy,
                        bx + bw - 1,
                        by + bh - yy,
                        if Fixed::from_int(yy) > Fixed::from_int(bh) * Fixed::from_ratio(8, 10) {
                            a[4]
                        } else {
                            GREEN[4]
                        },
                    );
                }
            }
        }
        live.number(
            status.x + 3,
            status.y + 3,
            ((j.position[2] * Fixed::from_int(120)).round()).trunc_to_int() as u32,
            5,
            GREEN[4],
        );
        live.number(
            status.x + 3,
            status.y + status.h - 7,
            ((flight.speed * Fixed::from_ratio(42, 10)).round()).trunc_to_int() as u32,
            3,
            GREEN[4],
        );
        instrument_glass(live, status);
    }

    pub(super) fn paint_attitude(
        &self,
        live: &mut Live<'_, '_>,
        flight: &Flight,
        prepared: &PreparedCockpit,
    ) {
        let clip = live.clip();
        let j = &flight.jets[0];
        let ball = self.ball;
        let (sn, cs) = prepared.roll;
        let per = Fixed::from_int(ball.r) / Fixed::from_int(34);
        let pitch = (j.pitch.to_degrees() * per * Fixed::from_ratio(9, 10)).clamp(
            Fixed::from_int(-ball.r) * Fixed::from_ratio(85, 100),
            Fixed::from_int(ball.r) * Fixed::from_ratio(85, 100),
        );
        let rung = (Fixed::from_int(5) * per * Fixed::from_int(2))
            .round()
            .max(Fixed::from_int(6))
            / Fixed::from_int(2);
        for dy in (-ball.r).max(clip[1] - ball.y)..=ball.r.min(clip[3] - ball.y - 1) {
            for dx in (-ball.r).max(clip[0] - ball.x)..=ball.r.min(clip[2] - ball.x - 1) {
                let d = Fixed::from_int(dx * dx + dy * dy);
                if d > Fixed::from_int(ball.r * ball.r) {
                    continue;
                }
                let bx = cs * Fixed::from_int(dx) - sn * Fixed::from_int(dy);
                let by = sn * Fixed::from_int(dx) + cs * Fixed::from_int(dy) - pitch;
                let mut color = if by > Fixed::ZERO {
                    if by > Fixed::from_int(ball.r) * Fixed::from_ratio(55, 100) {
                        rgba(
                            Fixed::from_int(120),
                            Fixed::from_int(70),
                            Fixed::from_int(30),
                        )
                    } else {
                        rgba(
                            Fixed::from_int(150),
                            Fixed::from_int(92),
                            Fixed::from_int(40),
                        )
                    }
                } else if by < Fixed::from_int(-ball.r) * Fixed::from_ratio(55, 100) {
                    rgba(
                        Fixed::from_int(70),
                        Fixed::from_int(140),
                        Fixed::from_int(224),
                    )
                } else {
                    rgba(
                        Fixed::from_int(110),
                        Fixed::from_int(176),
                        Fixed::from_int(240),
                    )
                };
                let k = (by / rung).round();
                if by.abs() < Fixed::from_ratio(6, 10)
                    || (k != Fixed::ZERO
                        && (by - k * rung).abs() < Fixed::from_ratio(55, 100)
                        && bx.abs()
                            < Fixed::from_int(ball.r)
                                * if (k).trunc_to_int() % 2 == 0 {
                                    Fixed::from_ratio(34, 100)
                                } else {
                                    Fixed::from_ratio(18, 100)
                                })
                    || (bx.abs() < Fixed::from_ratio(6, 10)
                        && by.abs() < Fixed::from_int(ball.r) * Fixed::from_ratio(7, 10))
                {
                    color = WHITE;
                }
                let rim = d / Fixed::from_int((ball.r * ball.r).max(1));
                let fall = if rim > Fixed::from_ratio(9, 10) {
                    Fixed::from_int(130)
                } else if rim > Fixed::from_ratio(74, 100) {
                    Fixed::from_int(80)
                } else if rim > Fixed::from_ratio(55, 100) {
                    Fixed::from_int(34)
                } else {
                    Fixed::ZERO
                };
                color = blend(
                    color,
                    BLACK,
                    (fall
                        + (Fixed::from_int(dx + dy) / (Fixed::from_int(ball.r) * SQRT_2)
                            * Fixed::from_int(46))
                        .round())
                    .clamp(Fixed::ZERO, Fixed::from_int(200))
                        / Fixed::from_int(256),
                );
                live.put(ball.x + dx, ball.y + dy, color);
            }
        }
        let amber = rgba(
            Fixed::from_int(255),
            Fixed::from_int(206),
            Fixed::from_int(84),
        );
        for dy in (-ball.r).max(clip[1] - ball.y)..0.min(clip[3] - ball.y) {
            for dx in (-ball.r).max(clip[0] - ball.x)..0.min(clip[2] - ball.x) {
                let d =
                    (Fixed::from_int(dx * dx + dy * dy)).sqrt() / Fixed::from_int(ball.r.max(1));
                if d > Fixed::from_ratio(5, 10)
                    && d < Fixed::from_ratio(72, 100)
                    && (Fixed::from_int(dx))
                        < Fixed::from_int(dy) * Fixed::from_ratio(4, 10)
                            + Fixed::from_int(ball.r) * Fixed::from_ratio(1, 10)
                    && (Fixed::from_int(dx)) > Fixed::from_int(dy) * Fixed::from_ratio(24, 10)
                {
                    live.tint(
                        ball.x + dx,
                        ball.y + dy,
                        WHITE,
                        Fixed::from_int(54) / Fixed::from_int(256),
                    );
                }
            }
        }
        for side in [-1, 1] {
            let start = ball.x
                + side
                    * ((Fixed::from_int(ball.r) * Fixed::from_ratio(2, 10)).round()).trunc_to_int();
            let end = ball.x
                + side
                    * ((Fixed::from_int(ball.r) * Fixed::from_ratio(62, 100)).round())
                        .trunc_to_int();
            live.line(
                [Fixed::from_int(start), Fixed::from_int(ball.y)],
                [Fixed::from_int(end), Fixed::from_int(ball.y)],
                amber,
            );
        }
        live.rect(ball.x - 1, ball.y - 1, ball.x + 2, ball.y + 2, amber);
        live.put(
            ball.x,
            ball.y,
            rgba(
                Fixed::from_int(255),
                Fixed::from_int(244),
                Fixed::from_int(190),
            ),
        );
    }

    pub(super) fn paint_dials(&self, live: &mut Live<'_, '_>, prepared: &PreparedCockpit) {
        let b = &self.inks[1..=9];
        let a = &self.inks[17..=22];
        for (dial, end) in self.dials.iter().zip(prepared.needle_ends) {
            live.line(
                [
                    Fixed::from_int(dial.x) + Fixed::ONE,
                    Fixed::from_int(dial.y) + Fixed::ONE,
                ],
                [end[0] + Fixed::ONE, end[1] + Fixed::ONE],
                BLACK,
            );
            live.line(
                [Fixed::from_int(dial.x), Fixed::from_int(dial.y)],
                end,
                a[4],
            );
            live.put(dial.x, dial.y, b[7]);
            for k in 0..4 {
                let angle = Fixed::from_int(-150 + k * 12) * PI / Fixed::from_int(180);
                live.tint(
                    dial.x + ((angle.cos() * Fixed::from_int(dial.r - 5)).round()).trunc_to_int(),
                    dial.y + ((angle.sin() * Fixed::from_int(dial.r - 5)).round()).trunc_to_int(),
                    WHITE,
                    Fixed::from_int(110) / Fixed::from_int(256),
                );
            }
        }
    }

    pub(super) fn paint_controls(
        &self,
        canvas: &mut Canvas<'_>,
        flight: &Flight,
        [deck, lever, stick]: [[Fixed; 3]; 3],
    ) {
        self.paint_deck_lights(&mut Live::new(canvas, deck), flight.time);
        self.paint_throttle(&mut Live::new(canvas, lever), flight.boost);
        self.paint_stick(&mut Live::new(canvas, stick), flight);
    }

    fn paint_deck_lights(&self, live: &mut Live<'_, '_>, time: Fixed) {
        let lit = self.inks[4];
        for (i, key) in self.deck_keys.iter().enumerate() {
            if hash8(
                (i * 91) as u32
                    + (time
                        * (Fixed::from_ratio(9, 10)
                            + Fixed::from_int((i) as i32) * Fixed::from_ratio(31, 100)))
                    .trunc_to_int() as u32,
            ) <= 150
            {
                live.rect(key.x, key.y, key.x + key.w, key.y + key.h, lit);
            }
        }
    }

    fn paint_throttle(&self, live: &mut Live<'_, '_>, boost: Fixed) {
        let r = |v| scaled_pixel(v, self.scale);
        let b = &self.inks[1..=9];
        let a = &self.inks[17..=22];
        let throttle = self.width / 2 + r(-150);
        let yy = r(290)
            - ((Fixed::from_int(r(290) - r(270))
                * (Fixed::from_ratio(18, 100) + Fixed::from_ratio(82, 100) * boost)
                    .clamp(Fixed::ZERO, Fixed::ONE))
            .round())
            .trunc_to_int();
        for u in [-7, 10] {
            live.rect(
                throttle + r(u),
                yy + r(5),
                throttle + r(u + 3),
                self.height + 12,
                b[5],
            );
            live.rect(
                throttle + r(u + 3),
                yy + r(5),
                throttle + r(u + 4),
                self.height + 12,
                b[1],
            );
        }
        live.rect(
            throttle - r(11) - 1,
            yy - 1,
            throttle + r(28) + 1,
            yy + r(9) + 1,
            b[0],
        );
        live.rect(throttle - r(11), yy, throttle + r(28), yy + r(9), a[3]);
        live.rect(
            throttle - r(11),
            yy,
            throttle + r(28),
            yy + r(3).max(r(2).max(1)),
            a[5],
        );
        live.rect(
            throttle - r(11),
            yy + r(6),
            throttle + r(28),
            yy + r(9),
            a[2],
        );
    }

    fn paint_stick(&self, live: &mut Live<'_, '_>, flight: &Flight) {
        let r = |v| scaled_pixel(v, self.scale);
        let cx = self.width / 2;
        let b = &self.inks[1..=9];
        let a = &self.inks[17..=22];
        let top = r(253)
            - (((flight.climb_tilt / Fixed::from_ratio(5, 10)).clamp(-Fixed::ONE, Fixed::ONE)
                * Fixed::from_int(r(3)))
            .round())
            .trunc_to_int();
        let lean = (flight.jets[0].yaw_rate
            / flight.settings.agility.max(Fixed::from_ratio(1, 10)))
        .clamp(-Fixed::ONE, Fixed::ONE)
            * Fixed::from_int(r(9));
        let reach = self.height + r(40) - top;
        let head_height = r(14).max(r(7).max(1));
        let grip_height = r(17).max(r(8).max(1));
        let fold = r(5).max(r(3).max(1));
        let shaft_start = head_height + grip_height;
        let wide = r(12);
        let narrow = r(8);
        let pleat = r(3);
        let trim = pleat.max(r(2).max(1));
        let grooves = r(4).max(pleat.max(1));
        let row = |d: i32| {
            let center = ((Fixed::from_int(cx)
                + lean * (Fixed::ONE - Fixed::from_int(d) / Fixed::from_int(reach)))
            .round())
            .trunc_to_int();
            let half = if d < head_height {
                wide
            } else if d < shaft_start {
                narrow
            } else {
                wide + (d - shaft_start) / fold * pleat
            };
            (center, half)
        };
        let [_, clip_top, _, clip_bottom] = live.clip();
        let bottom = self.height + self.pad;
        let shadow_y = r(3);
        let shadow_x = r(5);
        for y in (top + r(4)).max(clip_top - shadow_y)..bottom.min(clip_bottom - shadow_y) {
            let (center, half) = row(y - top);
            let center = center + shadow_x;
            for x in center - half..=center + half {
                if (x + y) & 1 == 0 {
                    live.tint(
                        x,
                        y + shadow_y,
                        BLACK,
                        Fixed::from_int(150) / Fixed::from_int(256),
                    );
                }
            }
        }
        for y in top.max(clip_top)..bottom.min(clip_bottom) {
            let d = y - top;
            let (xx, half) = row(d);
            let trim_edges = if d < head_height {
                if d < 2 {
                    2 - d
                } else if d > head_height - 2 {
                    1
                } else {
                    0
                }
            } else if d >= shaft_start {
                i32::from((d - shaft_start) % fold == 0)
            } else {
                0
            };
            let half = half - trim_edges;
            for i in -half..=half {
                let f = Fixed::from_int(i) / Fixed::from_int(half.max(1));
                let color = if d < head_height {
                    if d == 0 {
                        b[5]
                    } else if d < trim {
                        b[6]
                    } else if d > head_height - trim {
                        b[1]
                    } else if f < -Fixed::from_ratio(8, 10) {
                        b[5]
                    } else if f > Fixed::from_ratio(78, 100) {
                        b[1]
                    } else if f > Fixed::from_ratio(3, 10) {
                        b[2]
                    } else {
                        b[3]
                    }
                } else if d < shaft_start {
                    let groove = (d - head_height) % grooves == trim;
                    if groove {
                        if f < -Fixed::from_ratio(5, 100) {
                            b[2]
                        } else {
                            b[0]
                        }
                    } else if f < -Fixed::from_ratio(8, 10) {
                        b[2]
                    } else if f < -Fixed::from_ratio(45, 100) {
                        b[5]
                    } else if f < -Fixed::from_ratio(5, 100) {
                        b[4]
                    } else if f < Fixed::from_ratio(45, 100) {
                        b[3]
                    } else if f < Fixed::from_ratio(8, 10) {
                        b[1]
                    } else {
                        b[0]
                    }
                } else {
                    let within = (d - shaft_start) % fold;
                    if within == 0 {
                        if f < Fixed::from_ratio(2, 10) {
                            b[4]
                        } else {
                            b[2]
                        }
                    } else if within == fold - 1 || f > Fixed::from_ratio(5, 10) {
                        b[0]
                    } else if f < -Fixed::from_ratio(55, 100) {
                        b[2]
                    } else {
                        b[1]
                    }
                };
                live.put(xx + i, y, color);
            }
            live.put(xx - half - 1, y, b[0]);
            live.put(xx + half + 1, y, b[0]);
        }
        let xx = ((Fixed::from_int(cx) + lean).round()).trunc_to_int();
        for side in [-1, 1] {
            let x = xx + side * r(6);
            live.disc(x, top + r(7) + 1, r(4).max(r(2).max(1)), b[0]);
            live.disc(
                x,
                top + r(7),
                r(3).max(r(2).max(1)),
                if side > 0 {
                    if flight.boost > Fixed::from_ratio(3, 10) {
                        a[5]
                    } else {
                        a[3]
                    }
                } else {
                    b[2]
                },
            );
            live.put(x - 1, top + r(7) - 1, if side > 0 { a[5] } else { b[6] });
        }
    }
}
