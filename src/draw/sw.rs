use crate::types::{Color, Rect};

use super::command::DrawCommand;
use super::renderer::Renderer;

pub struct SwRenderer<'a> {
    buf: &'a mut [u8],
    width: u32,
    height: u32,
}

impl<'a> SwRenderer<'a> {
    pub fn new(buf: &'a mut [u8], width: u32, height: u32) -> Self {
        Self { buf, width, height }
    }

    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, opa: u8) {
        let screen = Rect {
            x: 0,
            y: 0,
            w: self.width as u16,
            h: self.height as u16,
        };
        let Some(draw_area) = area.intersect(clip) else {
            return;
        };
        let Some(draw_area) = draw_area.intersect(&screen) else {
            return;
        };

        let a = ((color.a as u16) * (opa as u16) / 255) as u8;

        for row in 0..draw_area.h as i32 {
            let y = draw_area.y + row;
            for col in 0..draw_area.w as i32 {
                let x = draw_area.x + col;
                let idx = ((y as u32 * self.width + x as u32) * 4) as usize;
                if idx + 3 >= self.buf.len() {
                    continue;
                }
                if a == 255 {
                    self.buf[idx] = color.r;
                    self.buf[idx + 1] = color.g;
                    self.buf[idx + 2] = color.b;
                    self.buf[idx + 3] = 255;
                } else {
                    let inv = 255 - a as u16;
                    self.buf[idx] =
                        ((color.r as u16 * a as u16 + self.buf[idx] as u16 * inv) / 255) as u8;
                    self.buf[idx + 1] =
                        ((color.g as u16 * a as u16 + self.buf[idx + 1] as u16 * inv) / 255) as u8;
                    self.buf[idx + 2] =
                        ((color.b as u16 * a as u16 + self.buf[idx + 2] as u16 * inv) / 255) as u8;
                    self.buf[idx + 3] = 255;
                }
            }
        }
    }
}

impl Renderer for SwRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        if let DrawCommand::Fill {
            area,
            color,
            opa,
            radius: _,
        } = cmd
        {
            self.fill_rect(area, clip, color, *opa);
        }
    }

    fn flush(&mut self) {}
}
