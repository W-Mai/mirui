//! `web-canvas` Renderer — paints into a 2D `<canvas>` context.

#![cfg(target_arch = "wasm32")]

mod texture_pool;

use alloc::format;
use alloc::string::String;

use web_sys::CanvasRenderingContext2d;

use self::texture_pool::{TextureKey, TexturePool, new_pool};
use crate::app::RendererFactory;
use crate::draw::canvas::Canvas;
use crate::draw::command::DrawCommand;
use crate::draw::path::{Path, PathCmd};
use crate::draw::renderer::Renderer;
use crate::draw::texture::Texture;
use crate::surface::web_canvas::WebCanvasSurface;
use crate::types::{Color, Fixed, Point, Rect, Viewport};

pub struct WebCanvasRendererFactory {
    texture_pool: TexturePool,
}

impl WebCanvasRendererFactory {
    pub fn new() -> Self {
        Self {
            texture_pool: new_pool(),
        }
    }
}

impl Default for WebCanvasRendererFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl RendererFactory<WebCanvasSurface> for WebCanvasRendererFactory {
    type Renderer<'a>
        = WebCanvasRenderer<'a>
    where
        Self: 'a;

    fn make<'a>(
        &'a mut self,
        backend: &'a mut WebCanvasSurface,
        transform: &Viewport,
    ) -> WebCanvasRenderer<'a> {
        WebCanvasRenderer {
            factory: self,
            surface: backend,
            viewport: *transform,
        }
    }
}

pub struct WebCanvasRenderer<'a> {
    factory: &'a mut WebCanvasRendererFactory,
    surface: &'a mut WebCanvasSurface,
    viewport: Viewport,
}

impl WebCanvasRenderer<'_> {
    fn ctx(&self) -> &CanvasRenderingContext2d {
        self.surface.ctx()
    }

    fn scale(&self) -> f64 {
        self.viewport.scale().to_f32() as f64
    }

    /// Push a clip rect onto the context's state stack. Caller must
    /// pair with `ctx.restore()` after the draw.
    fn push_clip(&self, clip: &Rect) {
        let s = self.scale();
        let ctx = self.ctx();
        ctx.save();
        ctx.begin_path();
        ctx.rect(
            clip.x.to_f32() as f64 * s,
            clip.y.to_f32() as f64 * s,
            clip.w.to_f32() as f64 * s,
            clip.h.to_f32() as f64 * s,
        );
        ctx.clip();
    }

    fn pop_clip(&self) {
        self.ctx().restore();
    }

    fn set_fill(&self, color: &Color, opa: u8) {
        let ctx = self.ctx();
        ctx.set_global_alpha((color.a as f64 * opa as f64) / (255.0 * 255.0));
        ctx.set_fill_style_str(&css_color(color));
    }

    fn set_stroke(&self, color: &Color, width: Fixed, opa: u8) {
        let ctx = self.ctx();
        ctx.set_global_alpha((color.a as f64 * opa as f64) / (255.0 * 255.0));
        ctx.set_stroke_style_str(&css_color(color));
        ctx.set_line_width((width.to_f32() as f64 * self.scale()).max(1.0));
    }

    /// Walk a `Path` and translate it into Canvas 2D path operations.
    /// All coordinates are scaled to physical pixels here so the
    /// caller can chain `fill()` / `stroke()` directly.
    fn build_path(&self, path: &Path) {
        let ctx = self.ctx();
        let s = self.scale();
        ctx.begin_path();
        for cmd in &path.cmds {
            match cmd {
                PathCmd::MoveTo(p) => {
                    ctx.move_to(p.x.to_f32() as f64 * s, p.y.to_f32() as f64 * s);
                }
                PathCmd::LineTo(p) => {
                    ctx.line_to(p.x.to_f32() as f64 * s, p.y.to_f32() as f64 * s);
                }
                PathCmd::QuadTo { ctrl, end } => {
                    ctx.quadratic_curve_to(
                        ctrl.x.to_f32() as f64 * s,
                        ctrl.y.to_f32() as f64 * s,
                        end.x.to_f32() as f64 * s,
                        end.y.to_f32() as f64 * s,
                    );
                }
                PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                    ctx.bezier_curve_to(
                        ctrl1.x.to_f32() as f64 * s,
                        ctrl1.y.to_f32() as f64 * s,
                        ctrl2.x.to_f32() as f64 * s,
                        ctrl2.y.to_f32() as f64 * s,
                        end.x.to_f32() as f64 * s,
                        end.y.to_f32() as f64 * s,
                    );
                }
                PathCmd::Close => ctx.close_path(),
            }
        }
    }
}

impl Renderer for WebCanvasRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        use crate::types::TransformClass;

        let tf = cmd.transform();
        let (tx, ty) = match tf.classify() {
            TransformClass::Identity => (Fixed::ZERO, Fixed::ZERO),
            TransformClass::Translate => (tf.tx, tf.ty),
            other => unimplemented!(
                "web-canvas backend: transform class {:?} not yet handled — render_system should pre-project to a quad",
                other
            ),
        };

        match cmd {
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
                ..
            } => {
                let area = offset_rect(area, tx, ty);
                self.fill_rect(&area, clip, color, *radius, *opa);
            }
            DrawCommand::Border {
                area,
                width,
                radius,
                color,
                opa,
                ..
            } => {
                let area = offset_rect(area, tx, ty);
                self.stroke_rect(&area, clip, *width, color, *radius, *opa);
            }
            DrawCommand::Blit {
                pos, size, texture, ..
            } => {
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                let pos = offset_point(pos, tx, ty);
                self.blit(texture, &src_rect, pos, *size, clip);
            }
            DrawCommand::Line {
                p1,
                p2,
                width,
                color,
                opa,
                ..
            } => {
                let p1 = offset_point(p1, tx, ty);
                let p2 = offset_point(p2, tx, ty);
                self.draw_line(p1, p2, clip, *width, color, *opa);
            }
            DrawCommand::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                width,
                color,
                opa,
                ..
            } => {
                let center = offset_point(center, tx, ty);
                self.draw_arc(
                    center,
                    *radius,
                    *start_angle,
                    *end_angle,
                    clip,
                    *width,
                    color,
                    *opa,
                );
            }
            DrawCommand::FillPath {
                path, color, opa, ..
            } => {
                if tx == Fixed::ZERO && ty == Fixed::ZERO {
                    self.fill_path(path, clip, color, *opa);
                } else {
                    let translated = translate_path(path, tx, ty);
                    self.fill_path(&translated, clip, color, *opa);
                }
            }
            DrawCommand::Label {
                pos,
                text,
                color,
                opa,
                ..
            } => {
                let pos = offset_point(pos, tx, ty);
                self.draw_label(&pos, text, clip, color, *opa);
            }
        }
    }

    fn flush(&mut self) {
        Canvas::flush(self)
    }
}

impl Canvas for WebCanvasRenderer<'_> {
    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        self.push_clip(clip);
        self.set_fill(color, opa);
        let s = self.scale();
        let x = area.x.to_f32() as f64 * s;
        let y = area.y.to_f32() as f64 * s;
        let w = area.w.to_f32() as f64 * s;
        let h = area.h.to_f32() as f64 * s;
        let r = (radius.to_f32() as f64 * s).max(0.0);
        let ctx = self.ctx();
        if r <= 0.0 {
            ctx.fill_rect(x, y, w, h);
        } else {
            ctx.begin_path();
            // `round_rect` is Safari 16 / Firefox 113 / Chromium new —
            // older engines return `Err`; fall back to a square corner.
            if ctx
                .round_rect_with_f64(x, y, w, h, r.min(w * 0.5).min(h * 0.5))
                .is_err()
            {
                ctx.rect(x, y, w, h);
            }
            ctx.fill();
        }
        self.pop_clip();
    }

    fn fill_path(&mut self, path: &Path, clip: &Rect, color: &Color, opa: u8) {
        self.push_clip(clip);
        self.set_fill(color, opa);
        self.build_path(path);
        self.ctx().fill();
        self.pop_clip();
    }

    fn stroke_path(&mut self, path: &Path, clip: &Rect, width: Fixed, color: &Color, opa: u8) {
        self.push_clip(clip);
        self.set_stroke(color, width, opa);
        self.build_path(path);
        self.ctx().stroke();
        self.pop_clip();
    }

    fn blit(&mut self, src: &Texture, src_rect: &Rect, dst: Point, dst_size: Point, clip: &Rect) {
        let key = TextureKey::from(src);
        let handle = match self
            .factory
            .texture_pool
            .entry(key)
            .or_try_insert_with::<_, ()>(|| texture_pool::upload(src).ok_or(()))
        {
            Ok(h) => h,
            Err(_) => return,
        };
        if handle.is_invalid() {
            return;
        }

        self.push_clip(clip);
        let s = self.scale();
        let ctx = self.ctx();
        let result = ctx
            .draw_image_with_offscreen_canvas_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                &handle.canvas,
                src_rect.x.to_f32() as f64,
                src_rect.y.to_f32() as f64,
                src_rect.w.to_f32() as f64,
                src_rect.h.to_f32() as f64,
                dst.x.to_f32() as f64 * s,
                dst.y.to_f32() as f64 * s,
                dst_size.x.to_f32() as f64 * s,
                dst_size.y.to_f32() as f64 * s,
            );
        let _ = result;
        self.pop_clip();
    }

    fn clear(&mut self, area: &Rect, color: &Color) {
        let s = self.scale();
        let ctx = self.ctx();
        ctx.save();
        ctx.set_global_alpha(color.a as f64 / 255.0);
        ctx.set_fill_style_str(&css_color(color));
        ctx.fill_rect(
            area.x.to_f32() as f64 * s,
            area.y.to_f32() as f64 * s,
            area.w.to_f32() as f64 * s,
            area.h.to_f32() as f64 * s,
        );
        ctx.restore();
    }

    fn draw_label(&mut self, pos: &Point, text: &[u8], clip: &Rect, color: &Color, opa: u8) {
        let Ok(s) = core::str::from_utf8(text) else {
            return;
        };
        self.push_clip(clip);
        self.set_fill(color, opa);
        let ctx = self.ctx();
        // Match `mirui::draw::font::CHAR_H` so layout sizing matches
        // the sw backend; the actual glyph rendering is browser-best-effort.
        let scale = self.scale();
        let px = (crate::draw::font::CHAR_H as f64 * scale).max(1.0);
        ctx.set_font(&format!("{px}px monospace"));
        ctx.set_text_baseline("top");
        let _ = ctx.fill_text(
            s,
            pos.x.to_f32() as f64 * scale,
            pos.y.to_f32() as f64 * scale,
        );
        self.pop_clip();
    }

    fn flush(&mut self) {
        // Canvas 2D commits when the run-loop yields back to the browser.
    }
}

fn css_color(c: &Color) -> String {
    format!("rgb({}, {}, {})", c.r, c.g, c.b)
}

fn offset_rect(r: &Rect, tx: Fixed, ty: Fixed) -> Rect {
    Rect::new(r.x + tx, r.y + ty, r.w, r.h)
}

fn offset_point(p: &Point, tx: Fixed, ty: Fixed) -> Point {
    Point {
        x: p.x + tx,
        y: p.y + ty,
    }
}

fn translate_path(path: &Path, tx: Fixed, ty: Fixed) -> Path {
    let shift = |p: &Point| Point {
        x: p.x + tx,
        y: p.y + ty,
    };
    let mut out = Path::new();
    for cmd in &path.cmds {
        match cmd {
            PathCmd::MoveTo(p) => out.cmds.push(PathCmd::MoveTo(shift(p))),
            PathCmd::LineTo(p) => out.cmds.push(PathCmd::LineTo(shift(p))),
            PathCmd::QuadTo { ctrl, end } => out.cmds.push(PathCmd::QuadTo {
                ctrl: shift(ctrl),
                end: shift(end),
            }),
            PathCmd::CubicTo { ctrl1, ctrl2, end } => out.cmds.push(PathCmd::CubicTo {
                ctrl1: shift(ctrl1),
                ctrl2: shift(ctrl2),
                end: shift(end),
            }),
            PathCmd::Close => out.cmds.push(PathCmd::Close),
        }
    }
    out
}
