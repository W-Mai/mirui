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

    fn dpr(&self) -> f64 {
        self.viewport.scale().to_f32() as f64
    }

    /// Push a clip rect onto the context state stack. The clip lives
    /// in logical pixels, so it stays anchored to the screen even when
    /// the caller already pushed a widget transform onto `ctx`.
    /// `pop_clip` undoes both the clip and the transform restoration.
    fn push_clip(&self, clip: &Rect) {
        let ctx = self.ctx();
        let saved = ctx
            .get_transform()
            .expect("CanvasRenderingContext2d.getTransform");
        ctx.save();
        let d = self.dpr();
        ctx.set_transform(d, 0.0, 0.0, d, 0.0, 0.0)
            .expect("setTransform(dpr)");
        ctx.begin_path();
        ctx.rect(
            clip.x.to_f32() as f64,
            clip.y.to_f32() as f64,
            clip.w.to_f32() as f64,
            clip.h.to_f32() as f64,
        );
        ctx.clip();
        ctx.set_transform(
            saved.a(),
            saved.b(),
            saved.c(),
            saved.d(),
            saved.e(),
            saved.f(),
        )
        .expect("setTransform(restore)");
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
        ctx.set_line_width((width.to_f32() as f64).max(1.0));
    }

    /// Walk a `Path` and translate it into Canvas 2D path operations.
    /// Coordinates are logical; the active context transform applies
    /// the DPR + widget transform once for the whole frame's draw.
    fn build_path(&self, path: &Path) {
        let ctx = self.ctx();
        ctx.begin_path();
        for cmd in &path.cmds {
            match cmd {
                PathCmd::MoveTo(p) => {
                    ctx.move_to(p.x.to_f32() as f64, p.y.to_f32() as f64);
                }
                PathCmd::LineTo(p) => {
                    ctx.line_to(p.x.to_f32() as f64, p.y.to_f32() as f64);
                }
                PathCmd::QuadTo { ctrl, end } => {
                    ctx.quadratic_curve_to(
                        ctrl.x.to_f32() as f64,
                        ctrl.y.to_f32() as f64,
                        end.x.to_f32() as f64,
                        end.y.to_f32() as f64,
                    );
                }
                PathCmd::CubicTo { ctrl1, ctrl2, end } => {
                    ctx.bezier_curve_to(
                        ctrl1.x.to_f32() as f64,
                        ctrl1.y.to_f32() as f64,
                        ctrl2.x.to_f32() as f64,
                        ctrl2.y.to_f32() as f64,
                        end.x.to_f32() as f64,
                        end.y.to_f32() as f64,
                    );
                }
                PathCmd::Close => ctx.close_path(),
            }
        }
    }
}

impl Renderer for WebCanvasRenderer<'_> {
    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        // Push DPR × widget-transform onto the context so every Canvas
        // method below can stay in logical pixels. Restored at the end
        // of the dispatch — the next `draw` call rebuilds the matrix
        // from scratch for that command's transform.
        let tf = cmd.transform();
        let dpr = self.dpr();
        let ctx = self.ctx();
        ctx.save();
        ctx.set_transform(
            tf.m00.to_f32() as f64 * dpr,
            tf.m10.to_f32() as f64 * dpr,
            tf.m01.to_f32() as f64 * dpr,
            tf.m11.to_f32() as f64 * dpr,
            tf.tx.to_f32() as f64 * dpr,
            tf.ty.to_f32() as f64 * dpr,
        )
        .expect("setTransform");

        match cmd {
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
                ..
            } => {
                self.fill_rect(area, clip, color, *radius, *opa);
            }
            DrawCommand::Border {
                area,
                width,
                radius,
                color,
                opa,
                ..
            } => {
                self.stroke_rect(area, clip, *width, color, *radius, *opa);
            }
            DrawCommand::Blit {
                pos, size, texture, ..
            } => {
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                self.blit(texture, &src_rect, *pos, *size, clip);
            }
            DrawCommand::Line {
                p1,
                p2,
                width,
                color,
                opa,
                ..
            } => {
                self.draw_line(*p1, *p2, clip, *width, color, *opa);
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
                self.draw_arc(
                    *center,
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
                self.fill_path(path, clip, color, *opa);
            }
            DrawCommand::Label {
                pos,
                text,
                color,
                opa,
                ..
            } => {
                self.draw_label(pos, text, clip, color, *opa);
            }
        }

        self.ctx().restore();
    }

    fn flush(&mut self) {
        Canvas::flush(self)
    }
}

impl Canvas for WebCanvasRenderer<'_> {
    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        self.push_clip(clip);
        self.set_fill(color, opa);
        let x = area.x.to_f32() as f64;
        let y = area.y.to_f32() as f64;
        let w = area.w.to_f32() as f64;
        let h = area.h.to_f32() as f64;
        let r = radius.to_f32().max(0.0) as f64;
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
        let ctx = self.ctx();
        let result = ctx
            .draw_image_with_offscreen_canvas_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                &handle.canvas,
                src_rect.x.to_f32() as f64,
                src_rect.y.to_f32() as f64,
                src_rect.w.to_f32() as f64,
                src_rect.h.to_f32() as f64,
                dst.x.to_f32() as f64,
                dst.y.to_f32() as f64,
                dst_size.x.to_f32() as f64,
                dst_size.y.to_f32() as f64,
            );
        let _ = result;
        self.pop_clip();
    }

    fn clear(&mut self, area: &Rect, color: &Color) {
        let ctx = self.ctx();
        ctx.save();
        ctx.set_global_alpha(color.a as f64 / 255.0);
        ctx.set_fill_style_str(&css_color(color));
        ctx.fill_rect(
            area.x.to_f32() as f64,
            area.y.to_f32() as f64,
            area.w.to_f32() as f64,
            area.h.to_f32() as f64,
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
        let px = (crate::draw::font::CHAR_H as f64).max(1.0);
        ctx.set_font(&format!("{px}px monospace"));
        ctx.set_text_baseline("top");
        let _ = ctx.fill_text(s, pos.x.to_f32() as f64, pos.y.to_f32() as f64);
        self.pop_clip();
    }

    fn flush(&mut self) {
        // Canvas 2D commits when the run-loop yields back to the browser.
    }
}

fn css_color(c: &Color) -> String {
    format!("rgb({}, {}, {})", c.r, c.g, c.b)
}
