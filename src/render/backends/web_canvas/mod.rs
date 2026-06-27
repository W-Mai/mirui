//! `web-canvas` Renderer — paints into a 2D `<canvas>` context.

#![cfg(target_arch = "wasm32")]

mod texture_pool;

use alloc::format;
use alloc::string::String;

use web_sys::CanvasRenderingContext2d;

use self::texture_pool::{GlyphKey, GlyphPool, TextureKey, TexturePool, new_glyph_pool, new_pool};
use crate::render::backends::sw::SwRenderer;
use crate::render::canvas::Canvas;
use crate::render::command::DrawCommand;
use crate::render::factory::RendererFactory;
use crate::render::path::{Path, PathCmd};
use crate::render::renderer::Renderer;
use crate::render::texture::{AlphaMode, ColorFormat, Texture};
use crate::surface::web_canvas::WebCanvasSurface;
use crate::types::{Color, Fixed, Point, Rect, Viewport};

pub struct WebCanvasRendererFactory {
    texture_pool: TexturePool,
    glyph_pool: GlyphPool,
}

impl WebCanvasRendererFactory {
    pub fn new() -> Self {
        Self {
            texture_pool: new_pool(),
            glyph_pool: new_glyph_pool(),
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

    // Text extent for the offscreen buffer. Must mirror the sw label
    // path's advance accumulation at scale=1 (the buffer renders with
    // Viewport scale 1), or the buffer clips the text. +1px row so the
    // AA bottom edge survives. Placeholder until a layout engine lands.
    fn measure_text_extent(&self, font: &crate::render::font::Font, text: &str) -> (i32, i32) {
        use crate::render::font::GlyphKind;
        let requested = font.size.max(1);
        let mut w: i32 = 0;
        for ch in text.chars() {
            let Some(g) = font.glyph(ch, requested) else {
                continue;
            };
            w += match &g.kind {
                GlyphKind::Sdf { source_size, .. } => {
                    g.advance as i32 * requested as i32 / (*source_size as i32).max(1)
                }
                _ => g.advance as i32,
            };
        }
        let h = (font.metrics().line_height as i32).max(requested as i32) + 1;
        (w, h)
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

    /// Affine quads use `setTransform` + `roundRect`; perspective
    /// quads fall back to `Path::rounded_quad`'s cubic bezier
    /// approximation — Canvas 2D has no homography.
    fn fill_quad_inner(
        &mut self,
        q: &[Point; 4],
        area: &Rect,
        clip: &Rect,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        self.push_clip(clip);
        self.set_fill(color, opa);
        if let Some(m) = quad_to_affine(q, area) {
            let ctx = self.ctx();
            let dpr = self.dpr();
            ctx.save();
            ctx.set_transform(
                m.0 * dpr,
                m.1 * dpr,
                m.2 * dpr,
                m.3 * dpr,
                m.4 * dpr,
                m.5 * dpr,
            )
            .expect("setTransform");
            self.fill_axis_aligned(area, radius);
            ctx.restore();
        } else {
            self.build_path(&Path::rounded_quad(q, radius));
            self.ctx().fill();
        }
        self.pop_clip();
    }

    fn stroke_quad_inner(
        &mut self,
        q: &[Point; 4],
        area: &Rect,
        width: Fixed,
        clip: &Rect,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        self.push_clip(clip);
        self.set_stroke(color, width, opa);
        if let Some(m) = quad_to_affine(q, area) {
            let ctx = self.ctx();
            let dpr = self.dpr();
            ctx.save();
            ctx.set_transform(
                m.0 * dpr,
                m.1 * dpr,
                m.2 * dpr,
                m.3 * dpr,
                m.4 * dpr,
                m.5 * dpr,
            )
            .expect("setTransform");
            self.stroke_axis_aligned(area, radius);
            ctx.restore();
        } else {
            self.build_path(&Path::rounded_quad(q, radius));
            self.ctx().stroke();
        }
        self.pop_clip();
    }

    fn fill_axis_aligned(&self, area: &Rect, radius: Fixed) {
        let ctx = self.ctx();
        let x = area.x.to_f32() as f64;
        let y = area.y.to_f32() as f64;
        let w = area.w.to_f32() as f64;
        let h = area.h.to_f32() as f64;
        let r = radius.to_f32().max(0.0) as f64;
        if r <= 0.0 {
            ctx.fill_rect(x, y, w, h);
        } else {
            ctx.begin_path();
            if ctx
                .round_rect_with_f64(x, y, w, h, r.min(w * 0.5).min(h * 0.5))
                .is_err()
            {
                ctx.rect(x, y, w, h);
            }
            ctx.fill();
        }
    }

    fn stroke_axis_aligned(&self, area: &Rect, radius: Fixed) {
        let ctx = self.ctx();
        let x = area.x.to_f32() as f64;
        let y = area.y.to_f32() as f64;
        let w = area.w.to_f32() as f64;
        let h = area.h.to_f32() as f64;
        let r = radius.to_f32().max(0.0) as f64;
        ctx.begin_path();
        if r <= 0.0 {
            ctx.rect(x, y, w, h);
        } else if ctx
            .round_rect_with_f64(x, y, w, h, r.min(w * 0.5).min(h * 0.5))
            .is_err()
        {
            ctx.rect(x, y, w, h);
        }
        ctx.stroke();
    }

    /// Quad blit via an `MESH_N × MESH_N` affine triangle mesh —
    /// Canvas 2D has no homography, so subdivision approximates one.
    fn blit_quad_inner(&mut self, src: &Texture, q: &[Point; 4], clip: &Rect, opa: u8) {
        if opa == 0 {
            return;
        }
        const MESH_N: i32 = 8;

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
        let prev_alpha = ctx.global_alpha();
        ctx.set_global_alpha(opa as f64 / 255.0);
        let src_w = src.width as f64;
        let src_h = src.height as f64;
        // Quad index order matches `apply_rect`: 0=TL, 1=TR, 2=BR, 3=BL.
        let interp = |u: f64, v: f64| -> (f64, f64) {
            let q0x = q[0].x.to_f32() as f64;
            let q0y = q[0].y.to_f32() as f64;
            let q1x = q[1].x.to_f32() as f64;
            let q1y = q[1].y.to_f32() as f64;
            let q2x = q[2].x.to_f32() as f64;
            let q2y = q[2].y.to_f32() as f64;
            let q3x = q[3].x.to_f32() as f64;
            let q3y = q[3].y.to_f32() as f64;
            let top_x = q0x * (1.0 - u) + q1x * u;
            let top_y = q0y * (1.0 - u) + q1y * u;
            let bot_x = q3x * (1.0 - u) + q2x * u;
            let bot_y = q3y * (1.0 - u) + q2y * u;
            (top_x * (1.0 - v) + bot_x * v, top_y * (1.0 - v) + bot_y * v)
        };

        for j in 0..MESH_N {
            for i in 0..MESH_N {
                let u0 = i as f64 / MESH_N as f64;
                let v0 = j as f64 / MESH_N as f64;
                let u1 = (i + 1) as f64 / MESH_N as f64;
                let v1 = (j + 1) as f64 / MESH_N as f64;
                let s00 = (u0 * src_w, v0 * src_h);
                let s10 = (u1 * src_w, v0 * src_h);
                let s11 = (u1 * src_w, v1 * src_h);
                let s01 = (u0 * src_w, v1 * src_h);
                let d00 = interp(u0, v0);
                let d10 = interp(u1, v0);
                let d11 = interp(u1, v1);
                let d01 = interp(u0, v1);
                draw_textured_triangle(
                    ctx,
                    &handle.canvas,
                    src_w,
                    src_h,
                    s00,
                    s10,
                    s11,
                    d00,
                    d10,
                    d11,
                );
                draw_textured_triangle(
                    ctx,
                    &handle.canvas,
                    src_w,
                    src_h,
                    s00,
                    s11,
                    s01,
                    d00,
                    d11,
                    d01,
                );
            }
        }
        ctx.set_global_alpha(prev_alpha);
        self.pop_clip();
    }

    /// Walk a `Path` and translate it into Canvas 2D path operations.
    /// Coordinates are logical; the active context transform applies
    /// the DPR + widget transform once for the whole frame's draw.
    fn build_path(&self, path: &Path) {
        let ctx = self.ctx();
        ctx.begin_path();
        for cmd in path.cmds.iter() {
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
    fn sample_target_region(
        &self,
        _src: &Rect,
    ) -> Option<crate::render::texture::Texture<'static>> {
        None
    }

    fn modify_target_region(
        &mut self,
        _src: &Rect,
        _f: &mut dyn FnMut(&mut crate::render::texture::Texture),
    ) -> bool {
        false
    }

    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        // `quad: Some(q)` already bakes `cmd.transform()` into its
        // 4 points; multiplying again would warp twice. Quad branches
        // paint under DPR-only; the rest stack `dpr × widget_tf`.
        let dpr = self.dpr();
        let tf = cmd.transform();
        let has_quad = matches!(
            cmd,
            DrawCommand::Fill { quad: Some(_), .. }
                | DrawCommand::Border { quad: Some(_), .. }
                | DrawCommand::Blit { quad: Some(_), .. }
        );
        let ctx = self.ctx();
        ctx.save();
        if has_quad {
            ctx.set_transform(dpr, 0.0, 0.0, dpr, 0.0, 0.0)
                .expect("setTransform");
        } else {
            ctx.set_transform(
                tf.m00.to_f32() as f64 * dpr,
                tf.m10.to_f32() as f64 * dpr,
                tf.m01.to_f32() as f64 * dpr,
                tf.m11.to_f32() as f64 * dpr,
                tf.tx.to_f32() as f64 * dpr,
                tf.ty.to_f32() as f64 * dpr,
            )
            .expect("setTransform");
        }

        match cmd {
            DrawCommand::Fill {
                area,
                quad: Some(q),
                color,
                radius,
                opa,
                ..
            } => {
                self.fill_quad_inner(q, area, clip, color, *radius, *opa);
            }
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
                quad: Some(q),
                width,
                color,
                radius,
                opa,
                ..
            } => {
                self.stroke_quad_inner(q, area, *width, clip, color, *radius, *opa);
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
                quad: Some(q),
                texture,
                opa,
                ..
            } => {
                self.blit_quad_inner(texture, q, clip, *opa);
            }
            DrawCommand::Blit {
                pos,
                size,
                texture,
                opa,
                ..
            } => {
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                self.blit(texture, &src_rect, *pos, *size, clip, *opa);
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
                font,
                color,
                opa,
                ..
            } => {
                self.draw_label(pos, text, font, clip, color, *opa);
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

    fn blit(
        &mut self,
        src: &Texture,
        src_rect: &Rect,
        dst: Point,
        dst_size: Point,
        clip: &Rect,
        opa: u8,
    ) {
        if opa == 0 {
            return;
        }
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
        // Canvas 2D's `globalAlpha` persists across calls; `save/restore`
        // in `Renderer::draw` captures whatever the previous primitive
        // left, so blit has to set it explicitly each call.
        let prev_alpha = ctx.global_alpha();
        ctx.set_global_alpha(opa as f64 / 255.0);
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
        ctx.set_global_alpha(prev_alpha);
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

    fn draw_label(
        &mut self,
        pos: &Point,
        text: &str,
        font: &crate::render::font::Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        // fillText can't render mirui's SDF / coverage atlases, so
        // software-render glyphs into a text-extent-sized RGBA buffer
        // (transparent bg → coverage in alpha) and composite onto the
        // canvas. Sized to the text and keyed on content (not clip), so
        // resize reflows the blit position without rebuilding the glyph.
        let (tw, th) = self.measure_text_extent(font, text);
        if tw == 0 || th == 0 {
            return;
        }
        let mut text_hash: u64 = 0xcbf2_9ce4_8422_2325;
        for b in text.bytes() {
            text_hash ^= b as u64;
            text_hash = text_hash.wrapping_mul(0x100_0000_01b3);
        }
        let key = GlyphKey {
            text_hash,
            family_ptr: font.family.as_ptr() as usize,
            size: font.size,
            color: (color.r as u32) << 24
                | (color.g as u32) << 16
                | (color.b as u32) << 8
                | color.a as u32,
            opa,
            scale: self.viewport.scale().to_int().clamp(1, u16::MAX as i32) as u16,
        };
        let handle = match self
            .factory
            .glyph_pool
            .entry(key)
            .or_try_insert_with::<_, ()>(|| {
                let mut buf = alloc::vec![0u8; tw as usize * th as usize * 4];
                {
                    let mut tex =
                        Texture::new(&mut buf, tw as u16, th as u16, ColorFormat::RGBA8888);
                    tex.alpha_mode = AlphaMode::Blend;
                    let mut sw = SwRenderer::new(tex);
                    sw.viewport = Viewport::new(tw as u16, th as u16, Fixed::ONE);
                    let origin = Point {
                        x: Fixed::ZERO,
                        y: Fixed::ZERO,
                    };
                    let full = Rect {
                        x: Fixed::ZERO,
                        y: Fixed::ZERO,
                        w: Fixed::from_int(tw),
                        h: Fixed::from_int(th),
                    };
                    sw.draw_label(&origin, text, font, &full, color, opa);
                }
                // sw blends onto a transparent buffer, leaving premultiplied
                // alpha (edge rgb already scaled by coverage). put_image_data
                // wants straight alpha, so un-premultiply the AA edges or
                // draw_image scales them a second time, darkening the fringe.
                for px in buf.chunks_exact_mut(4) {
                    let a = px[3] as u32;
                    if a != 0 && a != 255 {
                        for c in &mut px[..3] {
                            *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                        }
                    }
                }
                let tmp = Texture::new(&mut buf, tw as u16, th as u16, ColorFormat::RGBA8888);
                texture_pool::upload(&tmp).ok_or(())
            }) {
            Ok(h) => h,
            Err(_) => return,
        };
        self.push_clip(clip);
        let ctx = self.ctx();
        let _ = ctx.draw_image_with_offscreen_canvas_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
            &handle.get().canvas,
            0.0,
            0.0,
            tw as f64,
            th as f64,
            pos.x.to_f32() as f64,
            pos.y.to_f32() as f64,
            tw as f64,
            th as f64,
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

/// Recover the 2D affine `(a, b, c, d, e, f)` (`setTransform` argument
/// order) that maps `area`'s four corners to `q`. Returns `None` for
/// perspective quads — the top and bottom edges no longer parallel /
/// equal-length, which an affine matrix can't reproduce.
fn quad_to_affine(q: &[Point; 4], area: &Rect) -> Option<(f64, f64, f64, f64, f64, f64)> {
    let q0x = q[0].x.to_f32() as f64;
    let q0y = q[0].y.to_f32() as f64;
    let q1x = q[1].x.to_f32() as f64;
    let q1y = q[1].y.to_f32() as f64;
    let q2x = q[2].x.to_f32() as f64;
    let q2y = q[2].y.to_f32() as f64;
    let q3x = q[3].x.to_f32() as f64;
    let q3y = q[3].y.to_f32() as f64;

    let top_dx = q1x - q0x;
    let top_dy = q1y - q0y;
    let bot_dx = q2x - q3x;
    let bot_dy = q2y - q3y;

    // 0.5 px tolerance — `apply_rect` rounds to integer points before
    // emitting, so a true affine quad rounds to within one pixel.
    const EPS: f64 = 0.5;
    if (top_dx - bot_dx).abs() > EPS || (top_dy - bot_dy).abs() > EPS {
        return None;
    }

    let w = area.w.to_f32() as f64;
    let h = area.h.to_f32() as f64;
    if w.abs() < 1e-6 || h.abs() < 1e-6 {
        return None;
    }

    let ax = area.x.to_f32() as f64;
    let ay = area.y.to_f32() as f64;
    let a = (q1x - q0x) / w;
    let b = (q1y - q0y) / w;
    let c = (q3x - q0x) / h;
    let d = (q3y - q0y) / h;
    let e = q0x - a * ax - c * ay;
    let f = q0y - b * ax - d * ay;
    Some((a, b, c, d, e, f))
}

#[allow(clippy::too_many_arguments)]
fn draw_textured_triangle(
    ctx: &CanvasRenderingContext2d,
    src_canvas: &web_sys::OffscreenCanvas,
    src_w: f64,
    src_h: f64,
    s0: (f64, f64),
    s1: (f64, f64),
    s2: (f64, f64),
    d0: (f64, f64),
    d1: (f64, f64),
    d2: (f64, f64),
) {
    let det = (s1.0 - s0.0) * (s2.1 - s0.1) - (s2.0 - s0.0) * (s1.1 - s0.1);
    if det.abs() < 1e-6 {
        return;
    }
    let inv = 1.0 / det;
    let a = ((d1.0 - d0.0) * (s2.1 - s0.1) - (d2.0 - d0.0) * (s1.1 - s0.1)) * inv;
    let c = ((d2.0 - d0.0) * (s1.0 - s0.0) - (d1.0 - d0.0) * (s2.0 - s0.0)) * inv;
    let e = d0.0 - a * s0.0 - c * s0.1;
    let b = ((d1.1 - d0.1) * (s2.1 - s0.1) - (d2.1 - d0.1) * (s1.1 - s0.1)) * inv;
    let d = ((d2.1 - d0.1) * (s1.0 - s0.0) - (d1.1 - d0.1) * (s2.0 - s0.0)) * inv;
    let f = d0.1 - b * s0.0 - d * s0.1;

    ctx.save();
    // Post-multiply onto the caller's `dpr × widget_tf` (don't replace).
    ctx.transform(a, b, c, d, e, f).expect("transform");
    ctx.begin_path();
    ctx.move_to(s0.0, s0.1);
    ctx.line_to(s1.0, s1.1);
    ctx.line_to(s2.0, s2.1);
    ctx.close_path();
    ctx.clip();
    let _ = ctx.draw_image_with_offscreen_canvas_and_dw_and_dh(src_canvas, 0.0, 0.0, src_w, src_h);
    ctx.restore();
}
