//! Resize-time metalwork layers and live flight instruments.

use super::{
    flight::Flight,
    jet,
    raster::{BAYER, Canvas, Palette, blend, rgba},
    world::{World, hash8},
};
use alloc::{vec, vec::Vec};
use core::f32::consts::{PI, SQRT_2, TAU};

const GREEN: [u32; 6] = [
    0xff0d1403, 0xff162205, 0xff28420a, 0xff4e9616, 0xff7ee246, 0xffd2ffba,
];
const WHITE: u32 = 0xffff_ffff;
const BLACK: u32 = 0xff00_0000;
const HUD: u32 = 0xffa4_ff78;
const GLASS: u32 = 0xffff_e2aa;
const GLINT: u32 = 0xffff_f6e8;
const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7],
    [2, 6, 2, 2, 7],
    [7, 1, 7, 4, 7],
    [7, 1, 7, 1, 7],
    [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7],
    [7, 4, 7, 5, 7],
    [7, 1, 1, 2, 2],
    [7, 5, 7, 5, 7],
    [7, 5, 7, 1, 7],
];

#[derive(Clone, Copy, Default)]
struct Box2 {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}
#[derive(Clone, Copy, Default)]
struct Dial {
    x: i32,
    y: i32,
    r: i32,
}

/// Only procedural instrument artwork uses these small pixel primitives.
trait Ink {
    fn put(&mut self, x: i32, y: i32, color: u32);
    fn get(&self, x: i32, y: i32) -> u32;
    fn tint(&mut self, x: i32, y: i32, color: u32, alpha: f32) {
        let old = self.get(x, y);
        if old != 0 {
            self.put(x, y, blend(old, color, alpha));
        }
    }
    fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) {
        for y in y0..y1 {
            for x in x0..x1 {
                self.put(x, y, color);
            }
        }
    }
    fn line(&mut self, a: [f32; 2], b: [f32; 2], color: u32) {
        let n = (b[0] - a[0]).abs().max((b[1] - a[1]).abs()).ceil().max(1.0) as i32;
        for i in 0..=n {
            let t = i as f32 / n as f32;
            self.put(
                (a[0] + (b[0] - a[0]) * t).round() as i32,
                (a[1] + (b[1] - a[1]) * t).round() as i32,
                color,
            );
        }
    }
    fn disc(&mut self, x: i32, y: i32, r: i32, color: u32) {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    self.put(x + dx, y + dy, color);
                }
            }
        }
    }
    fn plate(&mut self, r: Box2, colors: [u32; 3]) {
        self.rect(r.x, r.y, r.x + r.w, r.y + r.h, colors[0]);
        self.rect(r.x, r.y, r.x + r.w, r.y + 1, colors[1]);
        self.rect(r.x, r.y, r.x + 1, r.y + r.h, colors[1]);
        self.rect(r.x, r.y + r.h - 1, r.x + r.w, r.y + r.h, colors[2]);
        self.rect(r.x + r.w - 1, r.y, r.x + r.w, r.y + r.h, colors[2]);
    }
    fn well(&mut self, r: Box2, colors: [u32; 3]) {
        self.plate(r, [colors[0], colors[2], colors[1]]);
        self.rect(r.x, r.y, r.x + r.w, r.y + 2, colors[2]);
        self.rect(r.x, r.y, r.x + 2, r.y + r.h, colors[2]);
    }
    fn bolt(&mut self, x: i32, y: i32, lit: u32, dark: u32) {
        self.rect(x, y, x + 2, y + 2, dark);
        self.put(x, y, lit);
    }
    fn number(&mut self, x: i32, y: i32, value: u32, digits: usize, color: u32) {
        let mut divisor = 10u32.pow((digits - 1) as u32);
        for i in 0..digits {
            let n = (value / divisor % 10) as usize;
            for (row, bits) in DIGITS[n].iter().enumerate() {
                for col in 0..3 {
                    if bits & (4 >> col) != 0 {
                        self.put(x + i as i32 * 4 + col, y + row as i32, color);
                    }
                }
            }
            divisor = (divisor / 10).max(1);
        }
    }
}
struct Layer {
    pixels: Vec<u32>,
    width: i32,
    height: i32,
    pad: i32,
}
impl Layer {
    fn new(width: i32, height: i32, pad: i32) -> Self {
        Self {
            pixels: vec![0; ((width + pad * 2) * (height + pad * 2)) as usize],
            width: width + pad * 2,
            height: height + pad * 2,
            pad,
        }
    }
}
impl Ink for Layer {
    fn put(&mut self, x: i32, y: i32, color: u32) {
        let x = x + self.pad;
        let y = y + self.pad;
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = color;
        }
    }
    fn get(&self, x: i32, y: i32) -> u32 {
        let x = x + self.pad;
        let y = y + self.pad;
        if x >= 0 && y >= 0 && x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize]
        } else {
            0
        }
    }
}
struct Live<'a, 'b> {
    canvas: &'a mut Canvas<'b>,
    offset: [i32; 2],
}
impl Ink for Live<'_, '_> {
    fn put(&mut self, x: i32, y: i32, color: u32) {
        self.canvas
            .point(x + self.offset[0], y + self.offset[1], color);
    }
    fn get(&self, x: i32, y: i32) -> u32 {
        let x = x + self.offset[0];
        let y = y + self.offset[1];
        if x >= 0 && y >= 0 && x < self.canvas.width as i32 && y < self.canvas.height as i32 {
            self.canvas.get(y as usize * self.canvas.width + x as usize)
        } else {
            0
        }
    }
}

pub struct Cockpit {
    width: i32,
    height: i32,
    scale: f32,
    pad: i32,
    layers: [Layer; 4],
    body: [u32; 9],
    hot: [u32; 6],
    panel_top: i32,
    deck_top: i32,
    rear_bottom: i32,
    screens: [Box2; 2],
    ball: Dial,
    dials: [Dial; 4],
    lamps: [Box2; 5],
    panel_keys: Vec<Box2>,
    deck_keys: Vec<Box2>,
    top_jet: [u8; jet::WIDTH * jet::LENGTH],
}
impl Cockpit {
    pub fn bytes(&self) -> usize {
        self.layers
            .iter()
            .map(|l| l.pixels.capacity() * 4)
            .sum::<usize>()
            + (self.panel_keys.capacity() + self.deck_keys.capacity())
                * core::mem::size_of::<Box2>()
            + core::mem::size_of::<Self>()
    }
    pub fn new(width: usize, height: usize, palette: &Palette) -> Self {
        let w = width as i32;
        let h = height as i32;
        let s = height as f32 / 300.0;
        let cx = w / 2;
        let r = |v: i32| (v as f32 * s).round() as i32;
        let x = |u: i32| cx + r(u);
        let pad = (14.0 * s).ceil() as i32 + 3;
        let b = core::array::from_fn(|i| palette.jets[0][i + 1]);
        let a = core::array::from_fn(|i| palette.jets[0][i + 17]);
        let mut art = Self {
            width: w,
            height: h,
            scale: s,
            pad,
            layers: core::array::from_fn(|_| Layer::new(w, h, pad)),
            body: b,
            hot: a,
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
            top_jet: [0; jet::WIDTH * jet::LENGTH],
        };
        let mut cells = [0; jet::WIDTH * jet::LENGTH * jet::LAYERS];
        jet::decode(&mut cells);
        for layer in (0..jet::LAYERS).rev() {
            for (i, c) in cells
                [layer * jet::WIDTH * jet::LENGTH..(layer + 1) * jet::WIDTH * jet::LENGTH]
                .iter()
                .enumerate()
            {
                if *c != 0 && art.top_jet[i] == 0 {
                    art.top_jet[i] = 1 + ((*c - 1) >> 4).min(2);
                }
            }
        }
        let inside = |dx: f32, dy: f32, ax: f32, ay: f32| {
            (dx.abs() / ax).powf(2.5) + (dy.abs() / ay).powf(2.5)
        };
        // Padded rear frame and a segmented, paint-chipped canopy hoop.
        for yy in -pad..h + pad {
            let dy = 252.0 * s - yy as f32;
            if dy <= 0.0 {
                continue;
            }
            for xx in -pad..w + pad {
                let v = inside((xx - cx) as f32, dy, 280.0 * s, 276.0 * s);
                if v <= 1.0 {
                    continue;
                }
                let f = (v.powf(0.4) - 1.0) * 280.0 * s;
                let n = hash8((xx * 71 + yy * 409) as u32);
                let color = if f < 1.2 {
                    b[5]
                } else if f < 2.4 + s {
                    b[3]
                } else if f < 2.4 + 6.0 * s {
                    if n < 20 { b[1] } else { b[2] }
                } else if n > 248 {
                    b[2]
                } else {
                    b[1]
                };
                art.layers[3].put(xx, yy, color);
                art.rear_bottom = art.rear_bottom.max(yy);
            }
        }
        let bar = (13.0 * s).max(6.0);
        let rx = 226.0 * s;
        let ry = 228.0 * s;
        let oy = 236.0 * s;
        for yy in -pad..h + pad {
            let dy = oy - yy as f32;
            if dy <= 0.0 {
                continue;
            }
            for xx in -pad..w + pad {
                let dx = (xx - cx) as f32;
                let inner = inside(dx, dy, rx, ry);
                if inner <= 1.0 || inside(dx, dy, rx + bar, ry + bar) > 1.0 {
                    continue;
                }
                let f = ((inner.powf(0.4) - 1.0) / ((rx + bar) / rx - 1.0)).clamp(0.0, 1.0);
                let part = (dy / ry).atan2(dx / rx) / PI * 15.0;
                let section = part.floor() as i32;
                let along = part.fract();
                let n = hash8((xx * 131 + yy * 977) as u32);
                let seam = !(0.035..=0.965).contains(&along);
                let mut color = if seam {
                    b[0]
                } else if f < 0.08 {
                    b[2]
                } else if f < 0.2 {
                    b[6]
                } else if f < 0.34 {
                    b[5]
                } else if f < 0.66 {
                    if n < 14 { b[3] } else { b[4] }
                } else if f < 0.86 {
                    b[2]
                } else {
                    b[0]
                };
                if !seam && [1, 4, 7, 10, 13].contains(&section) && f > 0.14 && f < 0.86 {
                    let edge = along.min(1.0 - along) * 9.0 + (f - 0.14).min(0.86 - f) * 5.0;
                    if f32::from(n) > 70.0 - edge * 150.0 {
                        color = if f < 0.3 {
                            a[5]
                        } else if f < 0.42 {
                            a[4]
                        } else if f < 0.68 {
                            a[3]
                        } else {
                            a[2]
                        };
                    }
                } else if !seam && f > 0.24 && f < 0.8 && n > 249 {
                    color = a[2];
                }
                art.layers[0].put(xx, yy, color);
            }
        }
        for i in 1..15 {
            for off in [-0.09, 0.09] {
                let angle = (i as f32 + off) / 15.0 * PI;
                let m = 1.0 + bar / (2.0 * rx);
                let cosine = angle.cos();
                let xx =
                    (cx as f32 + cosine.signum() * cosine.abs().powf(0.8) * rx * m).round() as i32;
                let yy = (oy - angle.sin().abs().powf(0.8) * ry * m).round() as i32;
                if art.layers[0].get(xx, yy) != 0 {
                    art.layers[0].bolt(xx - 1, yy - 1, b[7], b[0]);
                }
            }
        }
        let ty = (oy - ry - bar).round() as i32;
        for yy in -pad..r(21) {
            let inset = (yy as f32 * 0.45).max(0.0).round() as i32;
            art.layers[0].rect(
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
        art.layers[0].rect(x(-14), ty + r(7), x(14), ty + r(13), b[0]);
        art.layers[0].rect(x(-12), ty + r(8), x(12), ty + r(12), a[4]);
        for side in [-1, 1] {
            let ly = r(118);
            let half = rx * (1.0 - ((oy - ly as f32) / ry).powf(2.5)).max(0.0).powf(0.4);
            let lx = (cx as f32 + side as f32 * (half + bar * 0.35)).round() as i32;
            art.layers[0].rect(lx - r(5), ly - r(9), lx + r(5), ly + r(9), b[0]);
            art.layers[0].rect(lx - r(4), ly - r(8), lx + r(4), ly + r(8), a[3]);
            art.layers[0].bolt(lx - 1, ly - 1, b[7], b[0]);
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
                            + (side as f32
                                * (u0 as f32 + (u1 - u0) as f32 * i as f32 / n as f32)
                                * s)
                                .round() as i32
                            + k * side;
                        let yy = ((v0 as f32 + (v1 - v0) as f32 * i as f32 / n as f32) * s).round()
                            as i32
                            + k;
                        if art.layers[0].get(xx, yy) == 0
                            && ((k == 0 && i.min(n - i) >= 2) || (xx + yy) & 1 == 0)
                        {
                            art.layers[0].put(xx, yy, GLINT);
                        }
                    }
                }
            }
        }
        let top_of = |u: f32| {
            let u = u.abs();
            if u <= 58.0 {
                168.0
            } else if u <= 74.0 {
                168.0 + (u - 58.0) / 16.0 * 11.0
            } else if u <= 92.0 {
                179.0
            } else if u <= 104.0 {
                179.0 - (u - 92.0) / 12.0 * 9.0
            } else if u <= 206.0 {
                170.0 + (u - 104.0) * 0.02
            } else {
                172.0 + (u - 206.0) / 16.0 * 10.0
            }
        };
        let g = r(11).max(5);
        for xx in -pad..w + pad {
            let u = (xx - cx) as f32 / s;
            if u.abs() > 232.0 {
                continue;
            }
            let top = (top_of(u) * s).round() as i32;
            art.panel_top = art.panel_top.min(top);
            for yy in top..h + pad {
                let d = yy - top;
                let n = hash8((xx * 53 + yy * 311) as u32);
                let mut color = if d < g {
                    let near = d as f32 / g as f32;
                    let ray = u.abs() / (15.0 + near * 9.0);
                    if d == 0 {
                        b[6]
                    } else if d == 1 {
                        b[4]
                    } else if ray.fract() < 0.07 {
                        b[0]
                    } else if near < 0.45 {
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
                    let v = yy as f32 / s;
                    if below < r(4) || (below < r(8) && (xx + yy) & 1 == 0) || v > 262.0 {
                        b[0]
                    } else if v > 250.0 {
                        if (xx + yy) & 1 == 0 { b[0] } else { b[1] }
                    } else if v > 222.0 || (xx + yy) & 3 == 0 {
                        b[1]
                    } else {
                        b[2]
                    }
                };
                if d > 0 && d < g / 2 && hash8(((xx >> 3) * 7 + 5) as u32) > 208 && n > 120 {
                    color = if d < 3 { a[3] } else { a[2] };
                }
                art.layers[1].put(xx, yy, color);
            }
        }
        for side in [-1, 1] {
            for i in 0..5 {
                art.layers[1].rect(
                    x(side * (150 + i * 5)),
                    r(173),
                    x(side * (150 + i * 5)) + r(2).max(1),
                    r(179),
                    b[0],
                );
            }
        }
        art.layers[1].well(
            Box2 {
                x: x(-34),
                y: r(183),
                w: r(68),
                h: r(8),
            },
            [b[0], b[4], b[0]],
        );
        for i in 0..5 {
            let lamp = Box2 {
                x: cx + ((-30.0 + i as f32 * 12.4) * s).round() as i32,
                y: r(185),
                w: r(10).max(4),
                h: r(5).max(3),
            };
            art.lamps[i] = lamp;
            art.layers[1].rect(lamp.x, lamp.y, lamp.x + lamp.w, lamp.y + lamp.h, a[1]);
        }
        for (i, u) in [-206, 106].into_iter().enumerate() {
            let x0 = x(u);
            let x1 = x(u + 100);
            let y0 = r(193);
            let y1 = r(261);
            let panel = &mut art.layers[1];
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
            let margin = r(7).max(4);
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
                GREEN[0],
            );
            art.screens[i] = screen;
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
                    x: screen.x + r(4) + k * (screen.w as f32 / 6.4).round() as i32,
                    y: y1 - r(7),
                    w: r(7).max(3),
                    h: r(3).max(2),
                };
                panel.plate(key, [b[5], b[7], b[0]]);
                if k == 1 || k == 4 {
                    art.panel_keys.push(key);
                }
            }
        }
        art.layers[1].plate(
            Box2 {
                x: x(-100),
                y: r(193),
                w: r(200),
                h: r(69),
            },
            [b[2], b[5], b[0]],
        );
        let ball = art.ball;
        let outer = ball.r + r(8).max(4);
        for dy in -outer - 3..=outer + 3 {
            for dx in -outer - 3..=outer + 3 {
                let d = ((dx * dx + dy * dy) as f32).sqrt();
                if d > (outer + 3) as f32 {
                    continue;
                }
                let color = if d > outer as f32 + 1.0 {
                    if ((dx + dy) as f32) < (outer as f32 * 0.4) {
                        continue;
                    }
                    b[0]
                } else if d > outer as f32 {
                    b[0]
                } else if d > ball.r as f32 + 2.0 {
                    let light = -(dx + dy) as f32 / (d * SQRT_2 + 0.001);
                    if light > 0.55 {
                        b[7]
                    } else if light > 0.1 {
                        b[5]
                    } else if light > -0.45 {
                        b[4]
                    } else {
                        b[2]
                    }
                } else {
                    b[0]
                };
                art.layers[1].put(ball.x + dx, ball.y + dy, color);
            }
        }
        for (i, (u, v)) in [(-72, 211), (-72, 246), (72, 211), (72, 246)]
            .into_iter()
            .enumerate()
        {
            let dial = Dial {
                x: x(u),
                y: r(v),
                r: r(12).max(6),
            };
            art.dials[i] = dial;
            for dy in -dial.r - 3..=dial.r + 4 {
                for dx in -dial.r - 3..=dial.r + 4 {
                    let d = ((dx * dx + dy * dy) as f32).sqrt();
                    if d > dial.r as f32 + 3.5 {
                        continue;
                    }
                    let color = if d > (dial.r + 2) as f32 {
                        if ((dx + dy) as f32) < (dial.r as f32 * 0.5) {
                            continue;
                        }
                        b[0]
                    } else if d > dial.r as f32 {
                        if -(dx + dy) as f32 > dial.r as f32 * 0.4 {
                            b[7]
                        } else if -(dx + dy) as f32 > -dial.r as f32 * 0.5 {
                            b[5]
                        } else {
                            b[2]
                        }
                    } else if d > dial.r as f32 - 1.5
                        || (d > (dial.r - 4) as f32 && -(dx + dy) as f32 > dial.r as f32 * 0.3)
                    {
                        b[0]
                    } else {
                        b[1]
                    };
                    art.layers[1].put(dial.x + dx, dial.y + dy, color);
                }
            }
            for mark in 0..10 {
                let angle = (-225 + mark * 30) as f32 * PI / 180.0;
                art.layers[1].put(
                    dial.x + (angle.cos() * (dial.r - 3) as f32).round() as i32,
                    dial.y + (angle.sin() * (dial.r - 3) as f32).round() as i32,
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
                        w: r(7).max(4),
                        h: r(5).max(3),
                    };
                    let lit = (i + j) % 2 == 0;
                    art.layers[1].plate(
                        key,
                        [
                            if lit { a[3] } else { b[4] },
                            if lit { a[5] } else { b[6] },
                            b[0],
                        ],
                    );
                    if lit && j == 0 {
                        art.panel_keys.push(key);
                    }
                }
            }
        }
        // The nearer consoles and sidewalls look down toward the pilot's seat.
        for xx in -pad..w + pad {
            let u = (xx - cx) as f32 / s;
            let abs = u.abs();
            let top = (if abs <= 212.0 {
                263.0
            } else if abs <= 222.0 {
                263.0 - (abs - 212.0) / 10.0 * 77.0
            } else {
                (186.0 - (abs - 222.0) * 0.42).max(110.0)
            } * s)
                .round() as i32;
            if abs <= 212.0 {
                art.deck_top = art.deck_top.min(top);
            }
            for yy in top..h + pad {
                let d = yy - top;
                let n = hash8((xx * 59 + yy * 211) as u32);
                let v = yy as f32 / s;
                let color = if abs > 212.0 {
                    let ray = (v - 112.0) / abs.max(1.0) * 7.0;
                    let rib = ray.fract() < 0.06;
                    if d == 0 {
                        b[6]
                    } else if d < r(3) {
                        b[4]
                    } else if d < r(5) {
                        b[2]
                    } else if d < r(7) {
                        b[0]
                    } else if abs < 222.0 {
                        if abs < 215.0 {
                            b[5]
                        } else if abs < 218.0 {
                            b[3]
                        } else {
                            b[1]
                        }
                    } else if rib {
                        b[0]
                    } else if ray.floor() as i32 % 2 == 0 {
                        if n > 250 { b[3] } else { b[2] }
                    } else {
                        b[1]
                    }
                } else if d == 0 {
                    b[6]
                } else if d == 1 {
                    b[4]
                } else if v > 290.0 {
                    if (xx + yy) & 1 == 0 { b[1] } else { b[2] }
                } else if n > 250 {
                    b[4]
                } else {
                    b[3]
                };
                art.layers[2].put(xx, yy, color);
            }
        }
        let key_colors = [
            a[3],
            a[3],
            b[4],
            rgba(255.0, 206.0, 84.0),
            rgba(255.0, 206.0, 84.0),
            b[4],
            b[4],
            a[3],
        ];
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
                        w: r(10).max(5),
                        h: r(6).max(3),
                    };
                    let color = key_colors[((if side < 0 {
                        i + j * 3
                    } else {
                        i * 2 + j * 3 + 2
                    }) % 8) as usize];
                    art.layers[2]
                        .plate(key, [color, if color == b[4] { b[6] } else { a[5] }, b[0]]);
                    if (side < 0 && i == 0) || (side > 0 && j == 1) {
                        art.deck_keys.push(key);
                    }
                }
            }
        }
        art.layers[2].well(
            Box2 {
                x: x(-173),
                y: r(267),
                w: r(64),
                h: h + pad - r(267),
            },
            [b[1], b[5], b[0]],
        );
        for u in [-158, -141, -124] {
            art.layers[2].rect(x(u), r(270), x(u) + r(3).max(2), h + pad, b[0]);
            for yy in (r(272)..h + pad).step_by(r(4).max(2) as usize) {
                art.layers[2].put(x(u) + r(3).max(2) + 1, yy, b[5]);
            }
        }
        for i in 0..4 {
            let tx = x(112 + i * 11);
            let up = i % 2 == 0;
            art.layers[2].rect(tx - 1, r(281), tx + r(6).max(4) + 1, r(288), b[0]);
            art.layers[2].rect(tx, r(282), tx + r(5).max(3), r(286), b[5]);
            art.layers[2].rect(
                tx + 1,
                r(if up { 273 } else { 284 }),
                tx + r(3).max(2),
                r(if up { 283 } else { 293 }),
                b[7],
            );
            art.layers[2].rect(
                tx + 1,
                r(if up { 273 } else { 290 }),
                tx + r(3).max(2),
                r(if up { 275 } else { 293 }),
                WHITE,
            );
        }
        for (u, v, radius, color) in [
            (167, 282, 8, b[0]),
            (166, 280, 8, b[1]),
            (166, 280, 7, b[5]),
            (166, 279, 5, a[3]),
            (165, 278, 2, a[5]),
        ] {
            art.layers[2].disc(x(u), r(v), r(radius).max(1), color);
        }
        for dy in -r(10)..=r(10) {
            for dx in -r(32)..=r(32) {
                let d = dx as f32 * dx as f32 / (r(32).max(1) as f32).powi(2)
                    + dy as f32 * dy as f32 / (r(10).max(1) as f32).powi(2);
                if d <= 1.0 {
                    art.layers[2].put(
                        cx + dx,
                        r(296) + dy,
                        if d > 0.8 {
                            if dy < 0 { b[1] } else { b[5] }
                        } else if d > 0.62 {
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
                    w: r(8).max(4),
                    h: r(6).max(3),
                };
                art.layers[2].plate(
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
        for (layer, top, salt) in [
            (1, art.panel_top + g + r(10), 3),
            (2, art.deck_top + r(4), 11),
        ] {
            let pixels = &mut art.layers[layer];
            let mut worn = Vec::new();
            let plate = |c| [b[1], b[2], b[3]].contains(&c);
            let edge = |c| [b[0], b[4], b[5], b[6]].contains(&c);
            for yy in top + pad..pixels.height - 1 {
                for xx in 1..pixels.width - 1 {
                    let at = (yy * pixels.width + xx) as usize;
                    if !plate(pixels.pixels[at])
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
                    .any(|i| edge(pixels.pixels[i]))
                        && hash8((xx * 7 + yy * 13) as u32) > 96
                    {
                        worn.push(at);
                    }
                }
            }
            for at in worn {
                let n = hash8(at as u32);
                pixels.pixels[at] = if n > 170 {
                    a[3]
                } else if n > 70 {
                    a[2]
                } else {
                    a[1]
                };
            }
        }
        art
    }

    pub fn draw(&self, canvas: &mut Canvas<'_>, world: &World, flight: &Flight, palette: &Palette) {
        let p = flight.projection(self.width as usize, self.height as usize);
        let amount = p.cockpit_in();
        if amount <= 0.0 {
            return;
        }
        let s = self.scale;
        let r = |v: i32| (v as f32 * s).round() as i32;
        let cx = self.width / 2;
        let j = &flight.jets[0];
        let b = self.body;
        let a = self.hot;
        let head = [
            flight.head[0].position.to_f32() * s,
            (flight.head[1].position.to_f32()
                + if flight.boost > 0.2 {
                    ((flight.time * 30.0) as i32 & 1) as f32 * 1.5 * flight.boost
                        - 0.75 * flight.boost
                } else {
                    0.0
                })
                * s,
        ];
        let away = 1.0 - amount;
        let deck_slide = away * (self.height - self.deck_top + 8) as f32 * 1.5;
        let offset = |near: f32, slide: f32| {
            let whole = slide.floor();
            [
                -(head[0] * near)
                    .round()
                    .clamp(-self.pad as f32, self.pad as f32),
                -(head[1] * near)
                    .round()
                    .clamp(-self.pad as f32, self.pad as f32)
                    + whole,
                slide - whole,
            ]
        };
        let canopy = offset(0.3, -away * self.height as f32 * 0.62);
        let glass = offset(0.5, away * (self.height - self.panel_top + 8) as f32);
        let panel = offset(0.72, away * (self.height - self.panel_top + 8) as f32);
        let deck = offset(1.0, deck_slide);
        let lever = offset(1.12, deck_slide);
        let stick = offset(1.32, deck_slide);
        let rear = offset(1.65, -away * self.height as f32 * 0.9);
        let light_length = (palette.sun[0].powi(2) + palette.sun[1].powi(2)).sqrt();
        let sun_side =
            (-palette.sun[0] * j.yaw.cos() - palette.sun[1] * j.yaw.sin()) / light_length;
        let sun_front =
            (-palette.sun[0] * j.yaw.sin() + palette.sun[1] * j.yaw.cos()) / light_length;
        let sun_power = match palette.day {
            super::raster::Daylight::Day => 1.0,
            super::raster::Daylight::Sunset => 1.1,
            super::raster::Daylight::Night => 0.45,
        };
        let light = Light {
            x: cx as f32 - sun_side * r(176) as f32,
            slope: (p.roll.tan() * 0.9 - sun_side * 0.6).clamp(-2.4, 2.4),
            half: r(78) as f32,
            alpha: 58.0 * sun_power * (0.72 - 0.28 * sun_front) / 256.0,
            color: match palette.day {
                super::raster::Daylight::Day => rgba(255.0, 246.0, 222.0),
                super::raster::Daylight::Sunset => rgba(255.0, 170.0, 92.0),
                super::raster::Daylight::Night => rgba(150.0, 190.0, 255.0),
            },
            top: self.panel_top,
        };
        self.lay(
            canvas,
            0,
            canopy,
            Some(Light {
                alpha: light.alpha * 0.5,
                ..light
            }),
        );
        self.hud(canvas, flight, glass);
        self.lay(canvas, 1, panel, Some(light));
        {
            let mut live = Live {
                canvas,
                offset: [panel[0] as i32, panel[1] as i32],
            };
            for (i, lamp) in self.lamps.iter().enumerate() {
                if flight.boost > 0.25
                    || i < 2
                    || (i == 2 && (flight.time * 2.0) as i32 % 2 == 0)
                    || (i == 4 && hash8((flight.time * 3.0) as u32 * 7) > 200)
                {
                    live.rect(lamp.x, lamp.y, lamp.x + lamp.w, lamp.y + lamp.h, a[4]);
                    live.rect(lamp.x, lamp.y, lamp.x + lamp.w, lamp.y + 1, a[5]);
                }
            }
            for (i, key) in self.panel_keys.iter().enumerate() {
                if hash8((i * 53) as u32 + (flight.time * (0.7 + i as f32 * 0.23)) as u32) <= 150 {
                    live.rect(key.x, key.y, key.x + key.w, key.y + key.h, b[3]);
                }
            }
            // Terrain radar, range rings, moving scan and formation contacts.
            let radar = self.screens[0];
            let radius = radar.w.min(radar.h) as f32 * 0.47;
            let zoom = 9.0 / s;
            let sweep = (flight.time * 1.6).rem_euclid(TAU);
            for yy in 0..radar.h {
                for xx in 0..radar.w {
                    let dx = xx - radar.w / 2;
                    let dy = yy - radar.h / 2;
                    let distance = ((dx * dx + dy * dy) as f32).sqrt();
                    let right = dx as f32 * zoom;
                    let ahead = -dy as f32 * zoom;
                    let mat = world.material(
                        (j.position[0] + right * j.yaw.cos() + ahead * j.yaw.sin()) as i32,
                        (j.position[1] + right * j.yaw.sin() - ahead * j.yaw.cos()) as i32,
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
                    if mat < 12
                        && [1.0, 0.66, 0.33]
                            .into_iter()
                            .any(|ring| (distance / radius - ring).abs() < 0.5 / radius)
                    {
                        tone = tone.max(if mat >= 4 { 4 } else { 2 });
                    }
                    if distance <= radius {
                        let angle = (sweep - (dx as f32).atan2(-dy as f32)).rem_euclid(TAU);
                        if angle < 0.12 {
                            tone = 5;
                        } else if angle < 0.7 && tone < 5 {
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
                    + ((ex * j.yaw.cos() + ey * j.yaw.sin()) / zoom).round() as i32;
                let y = radar.y + radar.h / 2
                    - ((ex * j.yaw.sin() - ey * j.yaw.cos()) / zoom).round() as i32;
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
            instrument_glass(&mut live, radar);
            // Aircraft status silhouette, two engine meters and numeric readouts.
            let status = self.screens[1];
            for y in 0..status.h {
                live.rect(
                    status.x,
                    status.y + y,
                    status.x + status.w,
                    status.y + y + 1,
                    GREEN[(y & 1) as usize],
                );
            }
            let fit = ((status.h - 8) as f32 / jet::LENGTH as f32)
                .min(status.w as f32 * 0.5 / jet::WIDTH as f32)
                .max(0.01);
            let jw = (jet::WIDTH as f32 * fit).round() as i32;
            let jh = (jet::LENGTH as f32 * fit).round() as i32;
            let jx = status.x + (status.w as f32 * 0.31).round() as i32 - jw / 2;
            let jy = status.y + (status.h - jh) / 2;
            let at = |xx: i32, yy: i32| {
                if xx < 0 || yy < 0 || xx >= jw || yy >= jh {
                    0
                } else {
                    self.top_jet[((yy as f32 / fit) as usize).min(jet::LENGTH - 1) * jet::WIDTH
                        + ((xx as f32 / fit) as usize).min(jet::WIDTH - 1)]
                }
            };
            for yy in 0..jh {
                for xx in 0..jw {
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
            if hash8((flight.time * 12.0) as u32) > 60 {
                for side in [-1, 1] {
                    let xx = jx + jw / 2 + side * 2;
                    live.rect(
                        xx - 1,
                        jy + jh,
                        xx + 1,
                        jy + jh + 1 + (flight.boost * 3.0).round() as i32,
                        a[5],
                    );
                }
            }
            let bh = status.h - 16;
            for engine in 0..2 {
                let bx = status.x + (status.w as f32 * 0.66).round() as i32 + engine * r(12).max(8);
                let by = status.y + 7;
                let bw = r(8).max(5);
                live.rect(bx - 1, by - 1, bx + bw + 1, by + bh + 1, GREEN[3]);
                live.rect(bx, by, bx + bw, by + bh, GREEN[0]);
                let level = (0.5
                    + 0.45 * flight.boost
                    + (f32::from(hash8((flight.time * 9.0) as u32 + engine as u32 * 31)) - 128.0)
                        / 2600.0)
                    .clamp(0.0, 1.0);
                for yy in 0..(bh as f32 * level).round() as i32 {
                    if yy % 3 != 2 {
                        live.rect(
                            bx + 1,
                            by + bh - 1 - yy,
                            bx + bw - 1,
                            by + bh - yy,
                            if yy as f32 > bh as f32 * 0.8 {
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
                (j.position[2] * 120.0).round() as u32,
                5,
                GREEN[4],
            );
            live.number(
                status.x + 3,
                status.y + status.h - 7,
                (flight.speed * 4.2).round() as u32,
                3,
                GREEN[4],
            );
            instrument_glass(&mut live, status);
            let ball = self.ball;
            let (sn, cs) = p.roll.sin_cos();
            let per = ball.r as f32 / 34.0;
            let pitch = (j.pitch.to_degrees() * per * 0.9)
                .clamp(-ball.r as f32 * 0.85, ball.r as f32 * 0.85);
            let rung = (5.0 * per * 2.0).round().max(6.0) / 2.0;
            for dy in -ball.r..=ball.r {
                for dx in -ball.r..=ball.r {
                    let d = (dx * dx + dy * dy) as f32;
                    if d > (ball.r * ball.r) as f32 {
                        continue;
                    }
                    let bx = cs * dx as f32 - sn * dy as f32;
                    let by = sn * dx as f32 + cs * dy as f32 - pitch;
                    let mut color = if by > 0.0 {
                        if by > ball.r as f32 * 0.55 {
                            rgba(120.0, 70.0, 30.0)
                        } else {
                            rgba(150.0, 92.0, 40.0)
                        }
                    } else if by < -ball.r as f32 * 0.55 {
                        rgba(70.0, 140.0, 224.0)
                    } else {
                        rgba(110.0, 176.0, 240.0)
                    };
                    let k = (by / rung).round();
                    if by.abs() < 0.6
                        || (k != 0.0
                            && (by - k * rung).abs() < 0.55
                            && bx.abs()
                                < ball.r as f32 * if k as i32 % 2 == 0 { 0.34 } else { 0.18 })
                        || (bx.abs() < 0.6 && by.abs() < ball.r as f32 * 0.7)
                    {
                        color = WHITE;
                    }
                    let rim = d / (ball.r * ball.r).max(1) as f32;
                    let fall = if rim > 0.9 {
                        130.0
                    } else if rim > 0.74 {
                        80.0
                    } else if rim > 0.55 {
                        34.0
                    } else {
                        0.0
                    };
                    color = blend(
                        color,
                        BLACK,
                        (fall + ((dx + dy) as f32 / (ball.r as f32 * SQRT_2) * 46.0).round())
                            .clamp(0.0, 200.0)
                            / 256.0,
                    );
                    live.put(ball.x + dx, ball.y + dy, color);
                }
            }
            let amber = rgba(255.0, 206.0, 84.0);
            for side in [-1, 1] {
                let start = ball.x + side * (ball.r as f32 * 0.2).round() as i32;
                let end = ball.x + side * (ball.r as f32 * 0.62).round() as i32;
                live.line(
                    [start as f32, ball.y as f32],
                    [end as f32, ball.y as f32],
                    amber,
                );
            }
            live.rect(ball.x - 1, ball.y - 1, ball.x + 2, ball.y + 2, amber);
            live.put(ball.x, ball.y, rgba(255.0, 244.0, 190.0));
            let needles = [
                -225.0
                    + (flight.speed
                        / (flight.settings.speed * flight.settings.boost * 1.08).max(1.0))
                    .clamp(0.0, 1.0)
                        * 270.0,
                -225.0
                    + (j.position[2] / flight.settings.ceiling.max(flight.settings.altitude))
                        .clamp(0.0, 1.0)
                        * 270.0,
                -90.0 + (flight.climb_tilt / 0.5).clamp(-1.0, 1.0) * 120.0,
                -90.0 + (j.yaw_rate / flight.settings.agility.max(0.1)).clamp(-1.0, 1.0) * 120.0,
            ];
            for (dial, degrees) in self.dials.iter().zip(needles) {
                let (sn, cs) = degrees.to_radians().sin_cos();
                let end = [
                    dial.x as f32 + cs * (dial.r - 3) as f32,
                    dial.y as f32 + sn * (dial.r - 3) as f32,
                ];
                live.line(
                    [dial.x as f32 + 1.0, dial.y as f32 + 1.0],
                    [end[0] + 1.0, end[1] + 1.0],
                    BLACK,
                );
                live.line([dial.x as f32, dial.y as f32], end, a[4]);
                live.put(dial.x, dial.y, b[7]);
                for k in 0..4 {
                    let angle = (-150 + k * 12) as f32 * PI / 180.0;
                    live.tint(
                        dial.x + (angle.cos() * (dial.r - 5) as f32).round() as i32,
                        dial.y + (angle.sin() * (dial.r - 5) as f32).round() as i32,
                        WHITE,
                        110.0 / 256.0,
                    );
                }
            }
        }
        self.lay(canvas, 2, deck, Some(light));
        {
            let mut live = Live {
                canvas,
                offset: [deck[0] as i32, deck[1] as i32],
            };
            for (i, key) in self.deck_keys.iter().enumerate() {
                if hash8((i * 91) as u32 + (flight.time * (0.9 + i as f32 * 0.31)) as u32) <= 150 {
                    live.rect(key.x, key.y, key.x + key.w, key.y + key.h, b[3]);
                }
            }
            live.offset = [lever[0] as i32, lever[1] as i32];
            let throttle = cx + r(-150);
            let yy = r(290)
                - ((r(290) - r(270)) as f32 * (0.18 + 0.82 * flight.boost).clamp(0.0, 1.0)).round()
                    as i32;
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
                yy + r(3).max(2),
                a[5],
            );
            live.rect(
                throttle - r(11),
                yy + r(6),
                throttle + r(28),
                yy + r(9),
                a[2],
            );
            live.offset = [stick[0] as i32, stick[1] as i32];
            let top =
                r(253) - ((flight.climb_tilt / 0.5).clamp(-1.0, 1.0) * r(3) as f32).round() as i32;
            let lean =
                (j.yaw_rate / flight.settings.agility.max(0.1)).clamp(-1.0, 1.0) * r(9) as f32;
            let reach = self.height + r(40) - top;
            let head_height = r(14).max(7);
            let grip_height = r(17).max(8);
            let fold = r(5).max(3);
            for y in top..self.height + self.pad {
                let d = y - top;
                let xx = (cx as f32 + lean * (1.0 - d as f32 / reach as f32)).round() as i32;
                let half = if d < head_height {
                    r(12)
                        - if d < 2 {
                            2 - d
                        } else if d > head_height - 2 {
                            1
                        } else {
                            0
                        }
                } else if d < head_height + grip_height {
                    r(8)
                } else {
                    r(12) + (d - head_height - grip_height) / fold * r(3)
                        - i32::from((d - head_height - grip_height) % fold == 0)
                };
                for i in -half..=half {
                    let f = i as f32 / half.max(1) as f32;
                    let color = if d < head_height {
                        if d == 0 {
                            b[5]
                        } else if d < r(3).max(2) {
                            b[6]
                        } else if d > head_height - r(3).max(2) {
                            b[1]
                        } else if f < -0.8 {
                            b[5]
                        } else if f > 0.78 {
                            b[1]
                        } else if f > 0.3 {
                            b[2]
                        } else {
                            b[3]
                        }
                    } else if d < head_height + grip_height {
                        let groove = (d - head_height) % r(4).max(3) == r(3).max(2);
                        if groove {
                            if f < -0.05 { b[2] } else { b[0] }
                        } else if f < -0.8 {
                            b[2]
                        } else if f < -0.45 {
                            b[5]
                        } else if f < -0.05 {
                            b[4]
                        } else if f < 0.45 {
                            b[3]
                        } else if f < 0.8 {
                            b[1]
                        } else {
                            b[0]
                        }
                    } else {
                        let within = (d - head_height - grip_height) % fold;
                        if within == 0 {
                            if f < 0.2 { b[4] } else { b[2] }
                        } else if within == fold - 1 || f > 0.5 {
                            b[0]
                        } else if f < -0.55 {
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
            let xx = (cx as f32 + lean).round() as i32;
            for side in [-1, 1] {
                let x = xx + side * r(6);
                live.disc(x, top + r(7) + 1, r(4).max(2), b[0]);
                live.disc(
                    x,
                    top + r(7),
                    r(3).max(2),
                    if side > 0 {
                        if flight.boost > 0.3 { a[5] } else { a[3] }
                    } else {
                        b[2]
                    },
                );
                live.put(x - 1, top + r(7) - 1, if side > 0 { a[5] } else { b[6] });
            }
        }
        self.lay(canvas, 3, rear, None);
    }

    fn lay(&self, canvas: &mut Canvas<'_>, layer: usize, offset: [f32; 3], light: Option<Light>) {
        let source = &self.layers[layer];
        let dx = offset[0] as i32;
        let dy = offset[1] as i32;
        let fraction = offset[2];
        for y in 0..self.height {
            let sy = y - dy + self.pad;
            if sy < 0 || sy >= source.height {
                continue;
            }
            for x in 0..self.width {
                let sx = x - dx + self.pad;
                if sx < 0 || sx >= source.width {
                    continue;
                }
                let at = (sy * source.width + sx) as usize;
                let mut color = source.pixels[at];
                let mut cover = 1.0;
                if fraction > 0.004 && sy > 0 {
                    let above = source.pixels[at - source.width as usize];
                    if above != color {
                        if above == 0 {
                            cover = 1.0 - fraction;
                        } else if color == 0 {
                            color = above;
                            cover = fraction;
                        } else {
                            color = blend(color, above, fraction);
                        }
                    }
                }
                if color == 0 {
                    continue;
                }
                if let Some(light) = light {
                    let mid = light.x + (y - light.top) as f32 * light.slope;
                    let xx = x as f32;
                    if xx >= mid - light.half
                        && xx < mid + light.half
                        && !(xx >= mid - light.half * 0.22 && xx < mid + light.half * 0.02)
                    {
                        let edge = (xx - mid + light.half).min(mid + light.half - xx);
                        if edge >= 4.0
                            || f32::from(BAYER[((y & 3) * 4 + (x & 3)) as usize]) <= edge * 4.0
                        {
                            color = blend(color, light.color, light.alpha);
                        }
                    }
                }
                let out = y as usize * canvas.width + x as usize;
                canvas.put(
                    out,
                    if cover == 1.0 {
                        color
                    } else {
                        blend(canvas.get(out), color, cover)
                    },
                );
            }
        }
    }

    fn hud(&self, canvas: &mut Canvas<'_>, flight: &Flight, offset: [f32; 3]) {
        let p = flight.projection(self.width as usize, self.height as usize);
        let alpha = super::flight::smooth((p.cockpit - 0.72) / 0.28);
        if alpha <= 0.0 {
            return;
        }
        let r = |v: i32| (v as f32 * self.scale).round() as i32;
        let cx = self.width / 2;
        let pivot = (self.height as f32 * 0.34).round() as i32;
        let gx = offset[0] as i32;
        let gy = offset[1] as i32;
        let top = pivot - r(57) + gy;
        let bottom = r(170) + gy;
        let edge = |y: i32| {
            r(76) as f32
                + (r(86) - r(76)) as f32 * (bottom - y) as f32 / (bottom - top).max(1) as f32
        };
        for y in top.max(0)..bottom.min(self.height) {
            let half = edge(y).round() as i32;
            let left = (cx + gx - half).max(0);
            let right = (cx + gx + half).min(self.width - 1);
            for x in left..=right {
                let at = y as usize * canvas.width + x as usize;
                let sheen = ((x - left) as f32) < r(10) as f32 + (y - top) as f32 * 0.12
                    && (x - left) as f32 > r(4) as f32 + (y - top) as f32 * 0.12;
                canvas.put(
                    at,
                    blend(
                        canvas.get(at),
                        GLASS,
                        if sheen { 30.0 } else { 10.0 } * alpha / 256.0,
                    ),
                );
            }
            if left > 0 {
                let at = y as usize * canvas.width + left as usize;
                canvas.put(at, blend(canvas.get(at), GLINT, 150.0 / 256.0 * alpha));
            }
            if right < self.width - 1 {
                let at = y as usize * canvas.width + right as usize;
                canvas.put(
                    at,
                    blend(canvas.get(at), self.body[5], 120.0 / 256.0 * alpha),
                );
            }
        }
        let mut hud = Hud {
            canvas,
            top,
            bottom,
            cx: cx + gx,
            top_half: r(86),
            bottom_half: r(76),
            alpha,
        };
        hud.line(
            [(cx + gx - r(86)) as f32, top as f32],
            [(cx + gx + r(86)) as f32, top as f32],
            GLINT,
        );
        let q = r(9);
        let l = r(4).max(2);
        for sx in [-1, 1] {
            for sy in [-1, 1] {
                hud.line(
                    [(cx + sx * q) as f32, (pivot + sy * q) as f32],
                    [(cx + sx * (q - l)) as f32, (pivot + sy * q) as f32],
                    HUD,
                );
                hud.line(
                    [(cx + sx * q) as f32, (pivot + sy * q) as f32],
                    [(cx + sx * q) as f32, (pivot + sy * (q - l)) as f32],
                    HUD,
                );
            }
        }
        hud.line(
            [(cx - r(3)) as f32, pivot as f32],
            [(cx + r(3)) as f32, pivot as f32],
            HUD,
        );
        hud.line(
            [cx as f32, (pivot - r(3)) as f32],
            [cx as f32, (pivot + r(3)) as f32],
            HUD,
        );
        let (sn, cs) = p.roll.sin_cos();
        let focal = p.scale / p.perspective.max(1e-6);
        for degrees in (-20_i32..=20).step_by(10) {
            let dy = p.center[1] - pivot as f32 - focal * (degrees as f32 * PI / 180.0 * 0.8).tan();
            for side in [-1, 1] {
                let xa = (side * r(if degrees == 0 { 16 } else { 8 })) as f32;
                let xb = (side * r(if degrees == 0 { 54 } else { 20 })) as f32;
                let a = [
                    cx as f32 + cs * xa + sn * dy,
                    pivot as f32 - sn * xa + cs * dy,
                ];
                let b = [
                    cx as f32 + cs * xb + sn * dy,
                    pivot as f32 - sn * xb + cs * dy,
                ];
                hud.line(a, b, HUD);
                if degrees != 0 {
                    let tick = dy + degrees.signum() as f32 * r(3) as f32;
                    hud.line(
                        b,
                        [
                            cx as f32 + cs * xb + sn * tick,
                            pivot as f32 - sn * xb + cs * tick,
                        ],
                        HUD,
                    );
                }
            }
        }
        let half = r(30);
        let step = r(5).max(3);
        for side in [-1, 1] {
            let x = cx + side * r(62);
            hud.line(
                [x as f32, (pivot - half) as f32],
                [x as f32, (pivot + half) as f32],
                HUD,
            );
            let value = if side < 0 {
                flight.speed * 0.6
            } else {
                flight.jets[0].position[2] * 0.8
            };
            let off = value.rem_euclid((step * 4) as f32);
            for yy in (-half - step * 4..=half).step_by(step as usize) {
                let y = (pivot as f32 + yy as f32 + off).round() as i32;
                if y < pivot - half || y > pivot + half {
                    continue;
                }
                let long = (yy + half + step * 4) / step % 4 == 0;
                hud.line(
                    [x as f32, y as f32],
                    [(x - side * r(if long { 6 } else { 3 })) as f32, y as f32],
                    HUD,
                );
            }
            let px = x + side * r(4);
            hud.line(
                [px as f32, pivot as f32],
                [(px + side * r(4)) as f32, (pivot - r(3)) as f32],
                HUD,
            );
            hud.line(
                [px as f32, pivot as f32],
                [(px + side * r(4)) as f32, (pivot + r(3)) as f32],
                HUD,
            );
        }
        hud.number(
            cx - r(62) - 6,
            pivot - half - 8,
            (flight.speed * 4.2).round() as u32,
            3,
            HUD,
        );
        hud.number(
            cx + r(62) - 10,
            pivot - half - 8,
            (flight.jets[0].position[2] * 120.0).round() as u32,
            5,
            HUD,
        );
        for i in 0..3 {
            let y = pivot - r(22) + i * r(4);
            let w = r(if i == 1 { 6 } else { 9 });
            if flight.boost > (2 - i) as f32 * 0.33 {
                hud.line(
                    [(cx - w) as f32, y as f32],
                    [(cx + w) as f32, y as f32],
                    HUD,
                );
            } else {
                hud.put(cx - w, y, HUD);
                hud.put(cx + w, y, HUD);
            }
        }
        let by = pivot + r(40);
        let span = r(34);
        hud.line(
            [(cx - span) as f32, by as f32],
            [(cx + span) as f32, by as f32],
            HUD,
        );
        let heading = flight.jets[0].yaw.to_degrees().rem_euclid(360.0);
        let per = r(4).max(2) as f32 / 5.0;
        for degrees in
            (((heading - 60.0) / 5.0).floor() as i32 * 5..=heading as i32 + 60).step_by(5)
        {
            let x = (cx as f32 + (degrees as f32 - heading) * per).round() as i32;
            if (x - cx).abs() <= span {
                hud.line(
                    [x as f32, by as f32],
                    [
                        x as f32,
                        (by - r(if degrees.rem_euclid(15) == 0 { 4 } else { 2 })) as f32,
                    ],
                    HUD,
                );
            }
        }
        hud.number(cx - 5, by - r(4) - 8, heading.round() as u32, 3, HUD);
    }
}

#[derive(Clone, Copy)]
struct Light {
    x: f32,
    slope: f32,
    half: f32,
    alpha: f32,
    color: u32,
    top: i32,
}
fn instrument_glass(ink: &mut impl Ink, r: Box2) {
    for y in 0..r.h {
        for x in 0..r.w {
            if x < 2 || y < 2 {
                ink.tint(
                    r.x + x,
                    r.y + y,
                    BLACK,
                    if x < 1 || y < 1 { 120.0 } else { 60.0 } / 256.0,
                );
            } else {
                let d = x as f32 + y as f32 * 0.9 - r.w as f32 * 0.34;
                if d > 0.0 && d < r.w as f32 * 0.16 {
                    ink.tint(r.x + x, r.y + y, WHITE, 22.0 / 256.0);
                } else if d > r.w as f32 * 0.24 && d < r.w as f32 * 0.28 {
                    ink.tint(r.x + x, r.y + y, WHITE, 16.0 / 256.0);
                }
            }
        }
    }
}
struct Hud<'a, 'b> {
    canvas: &'a mut Canvas<'b>,
    top: i32,
    bottom: i32,
    cx: i32,
    top_half: i32,
    bottom_half: i32,
    alpha: f32,
}
impl Ink for Hud<'_, '_> {
    fn put(&mut self, x: i32, y: i32, color: u32) {
        let edge = self.bottom_half as f32
            + (self.top_half - self.bottom_half) as f32 * (self.bottom - y) as f32
                / (self.bottom - self.top).max(1) as f32;
        if y < self.top
            || y >= self.bottom - 2
            || x < 0
            || y < 0
            || x >= self.canvas.width as i32
            || y >= self.canvas.height as i32
            || (x - self.cx).abs() as f32 > edge - 2.0
        {
            return;
        }
        let at = y as usize * self.canvas.width + x as usize;
        self.canvas
            .put(at, blend(self.canvas.get(at), color, self.alpha));
    }
    fn get(&self, x: i32, y: i32) -> u32 {
        if x >= 0 && y >= 0 && x < self.canvas.width as i32 && y < self.canvas.height as i32 {
            self.canvas.get(y as usize * self.canvas.width + x as usize)
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        flight::{Settings, View},
        raster::Raster,
    };
    use super::*;
    #[test]
    fn layered_cockpit_has_bounded_storage_and_live_instruments_allocate_nothing() {
        let world = World::new(7, 0.33);
        let mut flight = Flight::new(&world, Settings::default());
        flight.view = View::Cockpit;
        for _ in 0..300 {
            flight.step(&world, 0.016, 480, 300);
        }
        let mut raster = Raster::new(480, 300).unwrap();
        let art = Cockpit::new(480, 300, &raster.palette);
        assert!(art.bytes() < 3 * 1024 * 1024);
        assert!(art.layers.iter().all(|l| l.pixels.iter().any(|c| *c != 0)));
        raster.render(&world, &flight);
        let allocations = crate::test_support::tracked_allocations(|| {
            raster
                .frame
                .with_canvas(|canvas| art.draw(canvas, &world, &flight, &raster.palette))
        });
        assert_eq!(allocations, 0);
        let pixels = raster.frame.texture.read_pixels().unwrap();
        assert!(pixels.bytes().chunks_exact(4).all(|c| c[3] == 255));
    }
}
