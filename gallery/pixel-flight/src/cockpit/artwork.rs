use super::*;

const DESIGN_HEIGHT: i32 = 300;

fn arch_distance(offset: Fixed, radius: Fixed) -> Fixed {
    (offset.abs() / radius).pow_fraction(Fixed::from_ratio(25, 10))
}

impl Cockpit {
    fn panel_rim(&self) -> i32 {
        let r = |v| scaled_pixel(v, self.scale);
        r(11).max(r(5).max(1))
    }

    pub(crate) fn with_storage(width: usize, height: usize, compact: bool) -> Self {
        assert!(!compact || (width <= 128 && height <= 128));
        let w = width as i32;
        let h = height as i32;
        let s = Fixed::from_int(h) / Fixed::from_int(DESIGN_HEIGHT);
        let cx = w / 2;
        let r = |v: i32| scaled_pixel(v, s);
        let pad = ((Fixed::from_int(14) * s).ceil()).trunc_to_int() + 3;
        let mut art = Self {
            width: w,
            height: h,
            scale: s,
            pad,
            layers: core::array::from_fn(|_| Layer::new(w, h, pad, compact)),
            inks: INSTRUMENT_INKS,
            panel_top: h,
            deck_top: h,
            rear_bottom: 0,
            screens: [Box2::default(); 2],
            ball: Dial {
                x: cx,
                y: r(228),
                r: r(25),
            },
            dials: [Dial::default(); 4],
            lamps: [Box2::default(); 5],
            panel_keys: Vec::with_capacity(8),
            deck_keys: Vec::with_capacity(8),
            radar: Vec::new(),
        };
        art.build_rear(compact);
        art.build_canopy(compact);
        art.build_panel();
        art.build_panel_instruments();
        art.build_deck();
        art.seal_panels(compact);
        art.prepare_radar_grid();
        art
    }

    fn build_rear(&mut self, compact: bool) {
        let (w, h) = (self.width, self.height);
        let s = self.scale;
        let pad = self.pad;
        let cx = self.width / 2;
        let b = BODY_INKS;
        let rx = Fixed::from_int(280) * s;
        let ry = Fixed::from_int(276) * s;
        let center_y = Fixed::from_int(252) * s;
        for yy in -pad..h + pad {
            let dy = center_y - Fixed::from_int(yy);
            if dy <= Fixed::ZERO {
                continue;
            }
            let vertical = arch_distance(dy, ry);
            for xx in -pad..w + pad {
                let v = arch_distance(Fixed::from_int(xx - cx), rx) + vertical;
                if v <= Fixed::ONE {
                    continue;
                }
                let f = (v.pow_fraction(Fixed::from_ratio(4, 10)) - Fixed::ONE)
                    * Fixed::from_int(280)
                    * s;
                let n = hash8((xx * 71 + yy * 409) as u32);
                let color = if f < Fixed::from_ratio(12, 10) {
                    b[5]
                } else if f < Fixed::from_ratio(24, 10) + s {
                    b[3]
                } else if f < Fixed::from_ratio(24, 10) + Fixed::from_int(6) * s {
                    if n < 20 { b[1] } else { b[2] }
                } else if n > 248 {
                    b[2]
                } else {
                    b[1]
                };
                self.layers[3].put(xx, yy, color);
                self.rear_bottom = self.rear_bottom.max(yy);
            }
        }
        for i in 0..40 {
            let angle = Fixed::from_int(i) / Fixed::from_int(40) * PI;
            let (sn, cs) = angle.sin_cos();
            let m = Fixed::ONE + Fixed::from_int(9) / Fixed::from_int(280);
            let xx = ((Fixed::from_int(cx)
                + cs.signum()
                    * cs.abs().pow_fraction(Fixed::from_ratio(8, 10))
                    * Fixed::from_int(280)
                    * s
                    * m)
                .round())
            .trunc_to_int();
            let yy = ((Fixed::from_int(252) * s
                - sn.abs().pow_fraction(Fixed::from_ratio(8, 10)) * Fixed::from_int(276) * s * m)
                .round())
            .trunc_to_int();
            if self.layers[3].get(xx, yy) != 0 {
                self.layers[3].bolt(xx, yy, b[5], b[0]);
            }
        }
        self.layers[3].seal(compact);
    }

    fn build_canopy(&mut self, compact: bool) {
        let (w, h) = (self.width, self.height);
        let s = self.scale;
        let pad = self.pad;
        let cx = self.width / 2;
        let r = {
            let scale = self.scale;
            move |v| scaled_pixel(v, scale)
        };
        let x = |u| cx + r(u);
        let a = HOT_INKS;
        let b = BODY_INKS;
        let bar = (Fixed::from_int(13) * s).max(Fixed::from_int(6));
        let rx = Fixed::from_int(226) * s;
        let ry = Fixed::from_int(228) * s;
        let oy = Fixed::from_int(236) * s;
        let outer_rx = rx + bar;
        let outer_ry = ry + bar;
        let rim_scale = outer_rx / rx - Fixed::ONE;
        for yy in -pad..h + pad {
            let dy = oy - Fixed::from_int(yy);
            if dy <= Fixed::ZERO {
                continue;
            }
            let vertical = arch_distance(dy, ry);
            let outer_vertical = arch_distance(dy, outer_ry);
            let normalized_y = dy / ry;
            for xx in -pad..w + pad {
                let dx = Fixed::from_int(xx - cx);
                let inner = arch_distance(dx, rx) + vertical;
                if inner <= Fixed::ONE || arch_distance(dx, outer_rx) + outer_vertical > Fixed::ONE
                {
                    continue;
                }
                let f = ((inner.pow_fraction(Fixed::from_ratio(4, 10)) - Fixed::ONE) / rim_scale)
                    .clamp(Fixed::ZERO, Fixed::ONE);
                let part = normalized_y.atan2(dx / rx) / PI * Fixed::from_int(15);
                let section = part.floor().trunc_to_int();
                let along = part.fract();
                let n = hash8((xx * 131 + yy * 977) as u32);
                let seam =
                    !(Fixed::from_ratio(35, 1000)..=Fixed::from_ratio(965, 1000)).contains(&along);
                let mut color = if seam {
                    b[0]
                } else if f < Fixed::from_ratio(8, 100) {
                    b[2]
                } else if f < Fixed::from_ratio(2, 10) {
                    b[6]
                } else if f < Fixed::from_ratio(34, 100) {
                    b[5]
                } else if f < Fixed::from_ratio(66, 100) {
                    if n < 14 { b[3] } else { b[4] }
                } else if f < Fixed::from_ratio(86, 100) {
                    b[2]
                } else {
                    b[0]
                };
                if !seam
                    && [1, 4, 7, 10, 13].contains(&section)
                    && f > Fixed::from_ratio(14, 100)
                    && f < Fixed::from_ratio(86, 100)
                {
                    let edge = along.min(Fixed::ONE - along) * Fixed::from_int(9)
                        + (f - Fixed::from_ratio(14, 100)).min(Fixed::from_ratio(86, 100) - f)
                            * Fixed::from_int(5);
                    if Fixed::from_int((n) as i32)
                        > Fixed::from_int(70) - edge * Fixed::from_int(150)
                    {
                        color = if f < Fixed::from_ratio(3, 10) {
                            a[5]
                        } else if f < Fixed::from_ratio(42, 100) {
                            a[4]
                        } else if f < Fixed::from_ratio(68, 100) {
                            a[3]
                        } else {
                            a[2]
                        };
                    }
                } else if !seam
                    && f > Fixed::from_ratio(24, 100)
                    && f < Fixed::from_ratio(8, 10)
                    && n > 249
                {
                    color = a[2];
                }
                self.layers[0].put(xx, yy, color);
            }
        }
        for i in 1..15 {
            for off in [-Fixed::from_ratio(9, 100), Fixed::from_ratio(9, 100)] {
                let angle = (Fixed::from_int(i) + off) / Fixed::from_int(15) * PI;
                let m = Fixed::ONE + bar / (Fixed::from_int(2) * rx);
                let cosine = angle.cos();
                let xx = ((Fixed::from_int(cx)
                    + cosine.signum()
                        * cosine.abs().pow_fraction(Fixed::from_ratio(8, 10))
                        * rx
                        * m)
                    .round())
                .trunc_to_int();
                let yy = ((oy - angle.sin().abs().pow_fraction(Fixed::from_ratio(8, 10)) * ry * m)
                    .round())
                .trunc_to_int();
                if self.layers[0].get(xx, yy) != 0 {
                    self.layers[0].bolt(xx - 1, yy - 1, b[7], b[0]);
                }
            }
        }
        let ty = ((oy - ry - bar).round()).trunc_to_int();
        for yy in -pad..r(21) {
            let inset = ((Fixed::from_int(yy) * Fixed::from_ratio(45, 100))
                .max(Fixed::ZERO)
                .round())
            .trunc_to_int();
            self.layers[0].rect(
                cx - r(26) + inset,
                ty + yy,
                cx + r(26) - inset,
                ty + yy + 1,
                if yy < 2 {
                    b[5]
                } else if yy > r(21) - 3 {
                    b[1]
                } else {
                    b[3]
                },
            );
        }
        self.layers[0].rect(x(-14), ty + r(7), x(14), ty + r(13), b[0]);
        self.layers[0].rect(x(-12), ty + r(8), x(12), ty + r(12), a[4]);
        for side in [-1, 1] {
            let ly = r(118);
            let half = rx
                * (Fixed::ONE
                    - ((oy - Fixed::from_int(ly)) / ry).pow_fraction(Fixed::from_ratio(25, 10)))
                .max(Fixed::ZERO)
                .pow_fraction(Fixed::from_ratio(4, 10));
            let lx = ((Fixed::from_int(cx)
                + Fixed::from_int(side) * (half + bar * Fixed::from_ratio(35, 100)))
            .round())
            .trunc_to_int();
            self.layers[0].rect(lx - r(5), ly - r(9), lx + r(5), ly + r(9), b[0]);
            self.layers[0].rect(lx - r(4), ly - r(8), lx + r(4), ly + r(8), a[3]);
            self.layers[0].bolt(lx - 1, ly - 1, b[7], b[0]);
            for (u0, v0, u1, v1, wide) in [
                (-152_i32, 66, -124, 38, 3),
                (-141, 72, -127, 58, 2),
                (-190, 40, -170, 20, 3),
                (-197, 54, -184, 41, 2),
                (-178, 44, -172, 38, 1),
            ] {
                let n = r((u1 - u0).abs()).max(1);
                for i in 0..=n {
                    for k in 0..r(wide).max(1) {
                        let xx = cx
                            + ((Fixed::from_int(side)
                                * (Fixed::from_int(u0)
                                    + Fixed::from_int(u1 - u0) * Fixed::from_int(i)
                                        / Fixed::from_int(n))
                                * s)
                                .round())
                            .trunc_to_int()
                            + k * side;
                        let yy = (((Fixed::from_int(v0)
                            + Fixed::from_int(v1 - v0) * Fixed::from_int(i) / Fixed::from_int(n))
                            * s)
                            .round())
                        .trunc_to_int()
                            + k;
                        if self.layers[0].get(xx, yy) == 0
                            && ((k == 0 && i.min(n - i) >= 2) || (xx + yy) & 1 == 0)
                        {
                            self.layers[0].put(xx, yy, GLINT_INK);
                        }
                    }
                }
            }
        }
        self.layers[0].seal(compact);
    }

    fn build_panel(&mut self) {
        let (w, h) = (self.width, self.height);
        let s = self.scale;
        let pad = self.pad;
        let cx = self.width / 2;
        let r = {
            let scale = self.scale;
            move |v| scaled_pixel(v, scale)
        };
        let x = |u| cx + r(u);
        let a = HOT_INKS;
        let b = BODY_INKS;
        let g = self.panel_rim();
        let top_of = |u: Fixed| {
            let u = u.abs();
            if u <= Fixed::from_int(58) {
                Fixed::from_int(168)
            } else if u <= Fixed::from_int(74) {
                Fixed::from_int(168)
                    + (u - Fixed::from_int(58)) / Fixed::from_int(16) * Fixed::from_int(11)
            } else if u <= Fixed::from_int(92) {
                Fixed::from_int(179)
            } else if u <= Fixed::from_int(104) {
                Fixed::from_int(179)
                    - (u - Fixed::from_int(92)) / Fixed::from_int(12) * Fixed::from_int(9)
            } else if u <= Fixed::from_int(206) {
                Fixed::from_int(170) + (u - Fixed::from_int(104)) * Fixed::from_ratio(2, 100)
            } else {
                Fixed::from_int(172)
                    + (u - Fixed::from_int(206)) / Fixed::from_int(16) * Fixed::from_int(10)
            }
        };
        for xx in -pad..w + pad {
            let u = Fixed::from_int(xx - cx) / s;
            if u.abs() > Fixed::from_int(232) {
                continue;
            }
            let top = ((top_of(u) * s).round()).trunc_to_int();
            self.panel_top = self.panel_top.min(top);
            for yy in top..h + pad {
                let d = yy - top;
                let n = hash8((xx * 53 + yy * 311) as u32);
                let mut color = if d < g {
                    let near = Fixed::from_int(d) / Fixed::from_int(g);
                    let ray = u.abs() / (Fixed::from_int(15) + near * Fixed::from_int(9));
                    if d == 0 {
                        b[6]
                    } else if d == 1 {
                        b[4]
                    } else if ray.fract() < Fixed::from_ratio(7, 100) {
                        b[0]
                    } else if near < Fixed::from_ratio(45, 100) {
                        if n > 236 { b[3] } else { b[2] }
                    } else {
                        b[1]
                    }
                } else if d < g + 2 {
                    if d == g { b[5] } else { b[3] }
                } else if d < g + 4 {
                    b[0]
                } else {
                    let below = d - g - 4;
                    let v = Fixed::from_int(yy) / s;
                    if below < r(4)
                        || (below < r(8) && (xx + yy) & 1 == 0)
                        || v > Fixed::from_int(262)
                    {
                        b[0]
                    } else if v > Fixed::from_int(250) {
                        if (xx + yy) & 1 == 0 { b[0] } else { b[1] }
                    } else if v > Fixed::from_int(222) || (xx + yy) & 3 == 0 {
                        b[1]
                    } else {
                        b[2]
                    }
                };
                if d > 0 && d < g / 2 && hash8(((xx >> 3) * 7 + 5) as u32) > 208 && n > 120 {
                    color = if d < 3 { a[3] } else { a[2] };
                }
                self.layers[1].put(xx, yy, color);
            }
        }
        for side in [-1, 1] {
            for i in 0..5 {
                self.layers[1].rect(
                    x(side * (150 + i * 5)),
                    r(173),
                    x(side * (150 + i * 5)) + r(2).max(1),
                    r(179),
                    b[0],
                );
            }
        }
        self.layers[1].well(
            Box2 {
                x: x(-34),
                y: r(183),
                w: r(68),
                h: r(8),
            },
            [b[0], b[4], b[0]],
        );
    }

    fn build_panel_instruments(&mut self) {
        let s = self.scale;
        let r = |v| scaled_pixel(v, s);
        let cx = self.width / 2;
        let x = |u| cx + r(u);
        let a = HOT_INKS;
        let b = BODY_INKS;
        for i in 0..5 {
            let lamp = Box2 {
                x: cx
                    + (((-Fixed::from_int(30)
                        + Fixed::from_int((i) as i32) * Fixed::from_ratio(124, 10))
                        * s)
                        .round())
                    .trunc_to_int(),
                y: r(185),
                w: r(10).max(r(4).max(1)),
                h: r(5).max(r(3).max(1)),
            };
            self.lamps[i] = lamp;
            self.layers[1].rect(lamp.x, lamp.y, lamp.x + lamp.w, lamp.y + lamp.h, a[1]);
        }
        for (i, u) in [-206, 106].into_iter().enumerate() {
            let x0 = x(u);
            let x1 = x(u + 100);
            let y0 = r(193);
            let y1 = r(261);
            let panel = &mut self.layers[1];
            panel.rect(x0 + 2, y0 + 2, x1 + 3, y1 + 3, b[0]);
            panel.plate(
                Box2 {
                    x: x0,
                    y: y0,
                    w: x1 - x0,
                    h: y1 - y0,
                },
                [b[3], b[6], b[1]],
            );
            let margin = r(7).max(r(4).max(1));
            let screen = Box2 {
                x: x0 + margin,
                y: y0 + margin - 1,
                w: x1 - x0 - margin * 2,
                h: y1 - y0 - margin * 2 - r(3),
            };
            panel.well(
                Box2 {
                    x: screen.x - 3,
                    y: screen.y - 3,
                    w: screen.w + 6,
                    h: screen.h + 6,
                },
                [b[1], b[5], b[0]],
            );
            panel.rect(
                screen.x,
                screen.y,
                screen.x + screen.w,
                screen.y + screen.h,
                SCREEN_INK,
            );
            self.screens[i] = screen;
            for (bx, by) in [
                (x0 + 3, y0 + 3),
                (x1 - 5, y0 + 3),
                (x0 + 3, y1 - 5),
                (x1 - 5, y1 - 5),
            ] {
                panel.bolt(bx, by, b[7], b[0]);
            }
            for k in 0..6 {
                let key = Box2 {
                    x: screen.x
                        + r(4)
                        + k * ((Fixed::from_int(screen.w) / Fixed::from_ratio(64, 10)).round())
                            .trunc_to_int(),
                    y: y1 - r(7),
                    w: r(7).max(r(3).max(1)),
                    h: r(3).max(r(2).max(1)),
                };
                panel.plate(key, [b[5], b[7], b[0]]);
                if k == 1 || k == 4 {
                    self.panel_keys.push(key);
                }
            }
        }
        self.layers[1].plate(
            Box2 {
                x: x(-100),
                y: r(193),
                w: r(200),
                h: r(69),
            },
            [b[2], b[5], b[0]],
        );
        let ball = self.ball;
        let outer = ball.r + r(8).max(r(4).max(1));
        for dy in -outer - 3..=outer + 3 {
            for dx in -outer - 3..=outer + 3 {
                let d = (Fixed::from_int(dx * dx + dy * dy)).sqrt();
                if d > Fixed::from_int(outer + 3) {
                    continue;
                }
                let color = if d > Fixed::from_int(outer) + Fixed::ONE {
                    if (Fixed::from_int(dx + dy))
                        < (Fixed::from_int(outer) * Fixed::from_ratio(4, 10))
                    {
                        continue;
                    }
                    b[0]
                } else if d > Fixed::from_int(outer) {
                    b[0]
                } else if d > Fixed::from_int(ball.r) + Fixed::from_int(2) {
                    let light =
                        Fixed::from_int(-(dx + dy)) / (d * SQRT_2 + Fixed::from_ratio(1, 1000));
                    if light > Fixed::from_ratio(55, 100) {
                        b[7]
                    } else if light > Fixed::from_ratio(1, 10) {
                        b[5]
                    } else if light > -Fixed::from_ratio(45, 100) {
                        b[4]
                    } else {
                        b[2]
                    }
                } else {
                    b[0]
                };
                self.layers[1].put(ball.x + dx, ball.y + dy, color);
            }
        }
        let brass = BRASS_INKS;
        let rb = ball.r + r(5).max(r(3).max(1));
        for sx in [-1, 1] {
            for sy in [-1, 1] {
                let xx = (Fixed::from_int(ball.x)
                    + Fixed::from_int(sx) * Fixed::from_int(rb) / SQRT_2)
                    .round()
                    .trunc_to_int();
                let yy = (Fixed::from_int(ball.y)
                    + Fixed::from_int(sy) * Fixed::from_int(rb) / SQRT_2)
                    .round()
                    .trunc_to_int();
                self.layers[1].put(xx, yy, brass[2]);
                self.layers[1].put(xx + 1, yy, brass[1]);
                self.layers[1].put(xx, yy + 1, brass[1]);
                self.layers[1].put(xx + 1, yy + 1, brass[0]);
            }
        }
        let ph = r(5).max(r(3).max(1));
        for i in 0..ph {
            self.layers[1].rect(
                ball.x - i,
                ball.y - outer + ph - i,
                ball.x + i + 1,
                ball.y - outer + ph - i + 1,
                if i == 0 { brass[2] } else { brass[1] },
            );
        }
        let knob = &mut self.layers[1];
        knob.rect(
            ball.x - r(6),
            ball.y + outer - r(2),
            ball.x + r(6) + 2,
            ball.y + outer + r(4),
            b[0],
        );
        knob.rect(
            ball.x - r(6),
            ball.y + outer - r(3),
            ball.x + r(6) + 1,
            ball.y + outer + r(3),
            brass[1],
        );
        knob.rect(
            ball.x - r(6),
            ball.y + outer - r(3),
            ball.x + r(6) + 1,
            ball.y + outer - r(2),
            brass[2],
        );
        knob.rect(
            ball.x - r(6),
            ball.y + outer + r(2),
            ball.x + r(6) + 1,
            ball.y + outer + r(3),
            brass[0],
        );
        for i in (-r(5)..=r(5)).step_by(2) {
            knob.put(ball.x + i, ball.y + outer, brass[0]);
        }
        for (i, (u, v)) in [(-72, 211), (-72, 246), (72, 211), (72, 246)]
            .into_iter()
            .enumerate()
        {
            let dial = Dial {
                x: x(u),
                y: r(v),
                r: r(12).max(r(6).max(1)),
            };
            self.dials[i] = dial;
            for dy in -dial.r - 3..=dial.r + 4 {
                for dx in -dial.r - 3..=dial.r + 4 {
                    let d = (Fixed::from_int(dx * dx + dy * dy)).sqrt();
                    if d > Fixed::from_int(dial.r) + Fixed::from_ratio(35, 10) {
                        continue;
                    }
                    let color = if d > Fixed::from_int(dial.r + 2) {
                        if (Fixed::from_int(dx + dy))
                            < (Fixed::from_int(dial.r) * Fixed::from_ratio(5, 10))
                        {
                            continue;
                        }
                        b[0]
                    } else if d > Fixed::from_int(dial.r) {
                        if Fixed::from_int(-(dx + dy))
                            > Fixed::from_int(dial.r) * Fixed::from_ratio(4, 10)
                        {
                            b[7]
                        } else if Fixed::from_int(-(dx + dy))
                            > Fixed::from_int(-dial.r) * Fixed::from_ratio(5, 10)
                        {
                            b[5]
                        } else {
                            b[2]
                        }
                    } else if d > Fixed::from_int(dial.r) - Fixed::from_ratio(15, 10)
                        || (d > Fixed::from_int(dial.r - 4)
                            && Fixed::from_int(-(dx + dy))
                                > Fixed::from_int(dial.r) * Fixed::from_ratio(3, 10))
                    {
                        b[0]
                    } else {
                        b[1]
                    };
                    self.layers[1].put(dial.x + dx, dial.y + dy, color);
                }
            }
            for mark in 0..10 {
                let angle = Fixed::from_int(-225 + mark * 30) * PI / Fixed::from_int(180);
                self.layers[1].put(
                    dial.x + ((angle.cos() * Fixed::from_int(dial.r - 3)).round()).trunc_to_int(),
                    dial.y + ((angle.sin() * Fixed::from_int(dial.r - 3)).round()).trunc_to_int(),
                    if mark > 7 { a[4] } else { b[7] },
                );
            }
        }
        for side in [-1, 1] {
            for j in 0..2 {
                for i in 0..2 {
                    let key = Box2 {
                        x: x(side * 72) - r(8) + i * r(9),
                        y: r(224) + j * r(6),
                        w: r(7).max(r(4).max(1)),
                        h: r(5).max(r(3).max(1)),
                    };
                    let lit = (i + j) % 2 == 0;
                    self.layers[1].plate(
                        key,
                        [
                            if lit { a[3] } else { b[4] },
                            if lit { a[5] } else { b[6] },
                            b[0],
                        ],
                    );
                    if lit && j == 0 {
                        self.panel_keys.push(key);
                    }
                }
            }
        }
    }

    fn build_deck(&mut self) {
        let w = self.width;
        let h = self.height;
        let s = self.scale;
        let pad = self.pad;
        let cx = self.width / 2;
        let r = {
            let scale = self.scale;
            move |v| scaled_pixel(v, scale)
        };
        let x = |u| cx + r(u);
        let a = HOT_INKS;
        let b = BODY_INKS;
        for xx in -pad..w + pad {
            let u = Fixed::from_int(xx - cx) / s;
            let abs = u.abs();
            let top = ((if abs <= Fixed::from_int(212) {
                Fixed::from_int(263)
            } else if abs <= Fixed::from_int(222) {
                Fixed::from_int(263)
                    - (abs - Fixed::from_int(212)) / Fixed::from_int(10) * Fixed::from_int(77)
            } else {
                (Fixed::from_int(186) - (abs - Fixed::from_int(222)) * Fixed::from_ratio(42, 100))
                    .max(Fixed::from_int(110))
            } * s)
                .round())
            .trunc_to_int();
            if abs <= Fixed::from_int(212) {
                self.deck_top = self.deck_top.min(top);
            }
            for yy in top..h + pad {
                let d = yy - top;
                let n = hash8((xx * 59 + yy * 211) as u32);
                let v = Fixed::from_int(yy) / s;
                let color = if abs > Fixed::from_int(212) {
                    let ray = (v - Fixed::from_int(112)) / abs.max(Fixed::ONE) * Fixed::from_int(7);
                    let rib = ray.fract() < Fixed::from_ratio(6, 100);
                    if d == 0 {
                        b[6]
                    } else if d < r(3) {
                        b[4]
                    } else if d < r(5) {
                        b[2]
                    } else if d < r(7) {
                        b[0]
                    } else if abs < Fixed::from_int(222) {
                        if abs < Fixed::from_int(215) {
                            b[5]
                        } else if abs < Fixed::from_int(218) {
                            b[3]
                        } else {
                            b[1]
                        }
                    } else if rib {
                        b[0]
                    } else if (ray.floor()).trunc_to_int() % 2 == 0 {
                        if n > 250 { b[3] } else { b[2] }
                    } else {
                        b[1]
                    }
                } else if d == 0 {
                    b[6]
                } else if d == 1 {
                    b[4]
                } else if v > Fixed::from_int(290) {
                    if (xx + yy) & 1 == 0 { b[1] } else { b[2] }
                } else if n > 250 {
                    b[4]
                } else {
                    b[3]
                };
                self.layers[2].put(xx, yy, color);
            }
        }
        let key_colors = [a[3], a[3], b[4], KEY_INK, KEY_INK, b[4], b[4], a[3]];
        for j in 0..3 {
            for i in 0..3 {
                for side in [-1, 1] {
                    let key = Box2 {
                        x: x(if side < 0 {
                            -209 + i * 12
                        } else {
                            180 + i * 12
                        }),
                        y: r(269 + j * 9),
                        w: r(10).max(r(5).max(1)),
                        h: r(6).max(r(3).max(1)),
                    };
                    let color = key_colors[((if side < 0 {
                        i + j * 3
                    } else {
                        i * 2 + j * 3 + 2
                    }) % 8) as usize];
                    self.layers[2]
                        .plate(key, [color, if color == b[4] { b[6] } else { a[5] }, b[0]]);
                    if (side < 0 && i == 0) || (side > 0 && j == 1) {
                        self.deck_keys.push(key);
                    }
                }
            }
        }
        self.layers[2].well(
            Box2 {
                x: x(-173),
                y: r(267),
                w: r(64),
                h: h + pad - r(267),
            },
            [b[1], b[5], b[0]],
        );
        for u in [-158, -141, -124] {
            self.layers[2].rect(x(u), r(270), x(u) + r(3).max(r(2).max(1)), h + pad, b[0]);
            for yy in (r(272)..h + pad).step_by(r(4).max(r(2).max(1)) as usize) {
                self.layers[2].put(x(u) + r(3).max(r(2).max(1)) + 1, yy, b[5]);
            }
        }
        for i in 0..4 {
            let tx = x(112 + i * 11);
            let up = i % 2 == 0;
            self.layers[2].rect(tx - 1, r(281), tx + r(6).max(r(4).max(1)) + 1, r(288), b[0]);
            self.layers[2].rect(tx, r(282), tx + r(5).max(r(3).max(1)), r(286), b[5]);
            self.layers[2].rect(
                tx + 1,
                r(if up { 273 } else { 284 }),
                tx + r(3).max(r(2).max(1)),
                r(if up { 283 } else { 293 }),
                b[7],
            );
            self.layers[2].rect(
                tx + 1,
                r(if up { 273 } else { 290 }),
                tx + r(3).max(r(2).max(1)),
                r(if up { 275 } else { 293 }),
                WHITE_INK,
            );
        }
        for (u, v, radius, color) in [
            (167, 282, 8, b[0]),
            (166, 280, 8, b[1]),
            (166, 280, 7, b[5]),
            (166, 279, 5, a[3]),
            (165, 278, 2, a[5]),
        ] {
            self.layers[2].disc(x(u), r(v), r(radius).max(1), color);
        }
        let oil_x = r(32);
        let oil_y = r(10);
        let oil_center = r(296);
        let radius_x2 = Fixed::from_int(oil_x.max(1)).powi(2);
        let radius_y2 = Fixed::from_int(oil_y.max(1)).powi(2);
        for dy in -oil_y..=oil_y {
            let vertical = Fixed::from_int(dy) * Fixed::from_int(dy) / radius_y2;
            for dx in -oil_x..=oil_x {
                let d = Fixed::from_int(dx) * Fixed::from_int(dx) / radius_x2 + vertical;
                if d <= Fixed::ONE {
                    self.layers[2].put(
                        cx + dx,
                        oil_center + dy,
                        if d > Fixed::from_ratio(8, 10) {
                            if dy < 0 { b[1] } else { b[5] }
                        } else if d > Fixed::from_ratio(62, 100) {
                            b[0]
                        } else {
                            b[1]
                        },
                    );
                }
            }
        }
        for side in [-1, 1] {
            for j in 0..4 {
                let key = Box2 {
                    x: x(side * 233) - if side < 0 { r(8) } else { 0 },
                    y: r(212 + j * 15),
                    w: r(8).max(r(4).max(1)),
                    h: r(6).max(r(3).max(1)),
                };
                self.layers[2].plate(
                    key,
                    [
                        if j == 1 {
                            a[3]
                        } else if j == 2 {
                            key_colors[3]
                        } else {
                            b[4]
                        },
                        b[6],
                        b[0],
                    ],
                );
            }
        }
    }

    fn seal_panels(&mut self, compact: bool) {
        let pad = self.pad;
        let r = {
            let scale = self.scale;
            move |v| scaled_pixel(v, scale)
        };
        let a = HOT_INKS;
        let b = BODY_INKS;
        for (layer, top, salt) in [
            (1, self.panel_top + self.panel_rim() + r(10), 3),
            (2, self.deck_top + r(4), 11),
        ] {
            let pixels = &mut self.layers[layer];
            let mut worn = Vec::new();
            let plate = |c| [b[1], b[2], b[3]].contains(&c);
            let edge = |c| [b[0], b[4], b[5], b[6]].contains(&c);
            for yy in top + pad..pixels.height - 1 {
                for xx in 1..pixels.width - 1 {
                    let at = (yy * pixels.width + xx) as usize;
                    if !plate(pixels.ink_at(at))
                        || hash8(((xx >> 3) * 31 + (yy >> 2) * 17 + salt) as u32) < 234
                    {
                        continue;
                    }
                    if [
                        at - 1,
                        at + 1,
                        at - pixels.width as usize,
                        at + pixels.width as usize,
                    ]
                    .into_iter()
                    .any(|i| edge(pixels.ink_at(i)))
                        && hash8((xx * 7 + yy * 13) as u32) > 96
                    {
                        worn.push(at);
                    }
                }
            }
            for at in worn {
                let n = hash8(at as u32);
                pixels.set_ink_at(
                    at,
                    if n > 170 {
                        a[3]
                    } else if n > 70 {
                        a[2]
                    } else {
                        a[1]
                    },
                );
            }
            pixels.seal(compact);
        }
    }

    fn prepare_radar_grid(&mut self) {
        let radar = self.screens[0];
        if radar.w > 0 && radar.h > 0 {
            self.radar = Vec::with_capacity(radar.w as usize * radar.h as usize);
            for y in 0..radar.h {
                for x in 0..radar.w {
                    self.radar.push(RadarCell::new(x, y, radar.w, radar.h));
                }
            }
        }
    }
}
