//! `web-canvas` Renderer — paints into a 2D `<canvas>` context.

#![cfg(target_arch = "wasm32")]

mod texture_pool;

use alloc::format;
use alloc::string::String;

use wasm_bindgen::JsCast;
use web_sys::{CanvasGradient, CanvasRenderingContext2d, CanvasWindingRule};

use self::texture_pool::{GlyphPool, TextureKey, TexturePool, new_glyph_pool, new_pool};
use crate::render::backends::sw::SwRenderer;
use crate::render::canvas::{Canvas, Paint};
use crate::render::command::{CompositeMode, DrawCommand, PosedGlyphs};
use crate::render::factory::RendererFactory;
use crate::render::path::{Path, PathCmd};
use crate::render::projective_fallback::ProjectiveFallback;
use crate::render::raster::{LineCap, LineJoin};
use crate::render::renderer::{
    DrawRequest, ProjectiveDrawError, RenderError, RenderFeature, RenderRoute, Renderer,
};
use crate::render::texture::{AlphaMode, ColorFormat, Texture};
use crate::surface::web_canvas::WebCanvasSurface;
use crate::types::{Color, Fixed, Point, Rect, Transform, Transform3D, Viewport};

fn paint_color(paint: &Paint) -> Color {
    match paint {
        Paint::Color(color) => (*color).into(),
        Paint::LinearGradient(gradient) => gradient
            .stops
            .first()
            .map(|stop| stop.color.into())
            .unwrap_or(Color::rgba(0, 0, 0, 0)),
        Paint::RadialGradient(gradient) => gradient
            .stops
            .first()
            .map(|stop| stop.color.into())
            .unwrap_or(Color::rgba(0, 0, 0, 0)),
    }
}

pub struct WebCanvasRendererFactory {
    texture_pool: TexturePool,
    glyph_pool: GlyphPool,
    projective_fallback: Option<ProjectiveFallback>,
}

impl WebCanvasRendererFactory {
    pub fn new() -> Self {
        Self {
            texture_pool: new_pool(),
            glyph_pool: new_glyph_pool(),
            projective_fallback: None,
        }
    }

    pub fn with_projective_fallback(mut self, fallback: ProjectiveFallback) -> Self {
        self.projective_fallback = Some(fallback);
        self
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

struct GlyphRunDraw<'a> {
    pos: &'a Point,
    glyphs: &'a [textflow::shaping::PositionedGlyph],
    font: &'a crate::render::font::Font,
    transform: &'a Transform,
    clip: &'a Rect,
    color: &'a Color,
    opacity: u8,
}

struct PosedGlyphRunDraw<'a> {
    pos: &'a Point,
    glyphs: &'a [textflow::shaping::PositionedGlyph],
    frames: &'a [textflow::placement::GlyphFrame],
    font: &'a crate::render::font::Font,
    transform: &'a Transform,
    clip: &'a Rect,
    color: &'a Color,
    opacity: u8,
}

fn map_point(
    c: mirx::types::Point,
    units: mirx::scene::GradientUnits,
    bbox: Option<Rect>,
) -> (f64, f64) {
    match units {
        mirx::scene::GradientUnits::UserSpaceOnUse => (c.x.to_f32() as f64, c.y.to_f32() as f64),
        mirx::scene::GradientUnits::ObjectBoundingBox => {
            let b = bbox.unwrap_or(Rect::new(0, 0, Fixed::ONE, Fixed::ONE));
            let cx: Fixed = c.x.into();
            let cy: Fixed = c.y.into();
            (
                (b.x + cx * b.w).to_f32() as f64,
                (b.y + cy * b.h).to_f32() as f64,
            )
        }
    }
}

fn map_scalar_grad(
    v: mirx::types::Fixed,
    units: mirx::scene::GradientUnits,
    bbox: Option<Rect>,
) -> f64 {
    match units {
        mirx::scene::GradientUnits::UserSpaceOnUse => v.to_f32().max(0.0) as f64,
        mirx::scene::GradientUnits::ObjectBoundingBox => {
            let b = bbox.unwrap_or(Rect::new(0, 0, Fixed::ONE, Fixed::ONE));
            let vf: Fixed = v.into();
            (vf * b.w).to_f32().max(0.0) as f64
        }
    }
}

fn map_gradient_points(
    start: mirx::types::Point,
    end: mirx::types::Point,
    units: mirx::scene::GradientUnits,
    bbox: Option<Rect>,
) -> (f64, f64, f64, f64) {
    let (sx, sy) = map_point(start, units, bbox);
    let (ex, ey) = map_point(end, units, bbox);
    (sx, sy, ex, ey)
}

impl WebCanvasRenderer<'_> {
    fn ctx(&self) -> &CanvasRenderingContext2d {
        self.surface.ctx()
    }

    fn dpr(&self) -> f64 {
        self.viewport.scale().to_f32() as f64
    }

    /// `None` when OOB — `getImageData` would silently return transparent
    /// black for the out-of-canvas portion (W3C spec).
    fn physical_clip_rect(&self, src: &Rect) -> Option<Rect> {
        let phys = self.viewport.rect_to_physical(*src);
        let (pw, ph) = self.viewport.physical_size();
        let target = Rect {
            x: Fixed::ZERO,
            y: Fixed::ZERO,
            w: Fixed::from_int(pw as i32),
            h: Fixed::from_int(ph as i32),
        };
        phys.intersect(&target)
    }

    /// Push a clip rect onto the context state stack. The clip lives
    /// in logical pixels, so it stays anchored to the screen even when
    /// the caller already pushed a widget transform onto `ctx`.
    /// `pop_clip` undoes both the clip and the transform restoration.
    fn push_rect_clip(&self, clip: &Rect) {
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

    fn pop_rect_clip(&self) {
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

    fn set_paint_style(&self, paint: &Paint, opa: u8, bbox: Option<Rect>) {
        let ctx = self.ctx();
        match paint {
            Paint::Color(color) => self.set_fill(&(*color).into(), opa),
            Paint::LinearGradient(gradient) => {
                ctx.set_global_alpha(1.0);
                let (sx, sy, ex, ey) =
                    map_gradient_points(gradient.start, gradient.end, gradient.units, bbox);
                let grad = ctx.create_linear_gradient(sx, sy, ex, ey);
                add_gradient_stops(&grad, &gradient.stops, opa);
                ctx.set_fill_style_canvas_gradient(&grad);
            }
            Paint::RadialGradient(gradient) => {
                ctx.set_global_alpha(1.0);
                let (cx, cy) = map_point(gradient.center, gradient.units, bbox);
                let r = map_scalar_grad(gradient.radius, gradient.units, bbox);
                if let Ok(grad) = ctx.create_radial_gradient(cx, cy, 0.0, cx, cy, r) {
                    add_gradient_stops(&grad, &gradient.stops, opa);
                    ctx.set_fill_style_canvas_gradient(&grad);
                } else {
                    self.set_fill(&paint_color(paint), opa);
                }
            }
        }
    }

    fn set_stroke_style(
        &self,
        paint: &Paint,
        width: Fixed,
        opa: u8,
        cap: LineCap,
        join: LineJoin,
        miter_limit: Fixed,
        bbox: Option<Rect>,
    ) {
        let ctx = self.ctx();
        ctx.set_line_width((width.to_f32() as f64).max(1.0));
        ctx.set_line_cap(line_cap_str(cap));
        ctx.set_line_join(line_join_str(join));
        ctx.set_miter_limit(miter_limit.to_f32().max(0.0) as f64);
        match paint {
            Paint::Color(color) => self.set_stroke(&(*color).into(), width, opa),
            Paint::LinearGradient(gradient) => {
                ctx.set_global_alpha(1.0);
                let (sx, sy, ex, ey) =
                    map_gradient_points(gradient.start, gradient.end, gradient.units, bbox);
                let grad = ctx.create_linear_gradient(sx, sy, ex, ey);
                add_gradient_stops(&grad, &gradient.stops, opa);
                ctx.set_stroke_style_canvas_gradient(&grad);
            }
            Paint::RadialGradient(gradient) => {
                ctx.set_global_alpha(1.0);
                let (cx, cy) = map_point(gradient.center, gradient.units, bbox);
                let r = map_scalar_grad(gradient.radius, gradient.units, bbox);
                if let Ok(grad) = ctx.create_radial_gradient(cx, cy, 0.0, cx, cy, r) {
                    add_gradient_stops(&grad, &gradient.stops, opa);
                    ctx.set_stroke_style_canvas_gradient(&grad);
                } else {
                    self.set_stroke(&paint_color(paint), width, opa);
                }
            }
        }
    }

    /// Affine quads use `setTransform` + `roundRect`; perspective
    /// quads fall back to `Path::rounded_quad`'s cubic bezier
    /// approximation — Canvas 2D has no homography.
    fn fill_quad_inner(
        &mut self,
        q: &[Point; 4],
        area: &Rect,
        clip: &Rect,
        paint: &Paint,
        radius: Fixed,
        opa: u8,
    ) {
        self.push_rect_clip(clip);
        let color = paint_color(paint);
        self.set_fill(&color, opa);
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
        self.pop_rect_clip();
    }

    fn stroke_quad_inner(
        &mut self,
        q: &[Point; 4],
        area: &Rect,
        width: Fixed,
        clip: &Rect,
        paint: &Paint,
        radius: Fixed,
        opa: u8,
    ) {
        self.push_rect_clip(clip);
        let color = paint_color(paint);
        self.set_stroke(&color, width, opa);
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
        self.pop_rect_clip();
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

    /// Affine quads use a direct Canvas transform. Projective quads retain
    /// the legacy mesh approximation until the checked fallback owns them.
    fn blit_quad_inner(&mut self, src: &Texture, q: &[Point; 4], clip: &Rect, opa: u8) {
        if opa == 0 {
            return;
        }
        const MESH_N: i32 = 8;

        let transient_up;
        let pooled_handle;
        let canvas_ref: &web_sys::OffscreenCanvas = if src.transient {
            transient_up = match texture_pool::upload(src) {
                Some(up) => up,
                None => return,
            };
            &transient_up.canvas
        } else {
            let key = TextureKey::from(src);
            pooled_handle = match self
                .factory
                .texture_pool
                .entry(key)
                .or_try_insert_with::<_, ()>(|| texture_pool::upload(src).ok_or(()))
            {
                Ok(h) => h,
                Err(_) => return,
            };
            if pooled_handle.is_invalid() {
                return;
            }
            &pooled_handle.canvas
        };

        self.push_rect_clip(clip);
        let ctx = self.ctx();
        let prev_alpha = ctx.global_alpha();
        ctx.set_global_alpha(opa as f64 / 255.0);
        let src_w = src.width as f64;
        let src_h = src.height as f64;
        if let Some(m) = quad_to_affine(q, &Rect::new(0, 0, src.width, src.height)) {
            let dpr = self.dpr();
            ctx.set_transform(
                m.0 * dpr,
                m.1 * dpr,
                m.2 * dpr,
                m.3 * dpr,
                m.4 * dpr,
                m.5 * dpr,
            )
            .expect("setTransform");
            let _ = ctx
                .draw_image_with_offscreen_canvas_and_dw_and_dh(canvas_ref, 0.0, 0.0, src_w, src_h);
            ctx.set_global_alpha(prev_alpha);
            self.pop_rect_clip();
            return;
        }
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
                draw_textured_triangle(ctx, canvas_ref, src_w, src_h, s00, s10, s11, d00, d10, d11);
                draw_textured_triangle(ctx, canvas_ref, src_w, src_h, s00, s11, s01, d00, d11, d01);
            }
        }
        ctx.set_global_alpha(prev_alpha);
        self.pop_rect_clip();
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

impl WebCanvasRenderer<'_> {
    fn classify_request(request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        use crate::types::TransformClass;

        let projected = !request.projective.is_identity();
        match request.command {
            DrawCommand::ApplyBlur { .. } => {
                return Err(RenderError::Unsupported(RenderFeature::Blur));
            }
            DrawCommand::FillPath { paint, .. } | DrawCommand::StrokePath { paint, .. }
                if !projected && !matches!(paint, Paint::Color(_)) =>
            {
                return Err(RenderError::Unsupported(RenderFeature::GradientPaint));
            }
            DrawCommand::StrokePath { dash, .. } if !projected && !dash.is_empty() => {
                return Err(RenderError::Unsupported(RenderFeature::StrokeStyle));
            }
            DrawCommand::Blit {
                quad,
                texture,
                radius,
                composite,
                ..
            } if !projected => {
                if let Some(q) = quad {
                    if *radius != Fixed::ZERO {
                        return Err(RenderError::Unsupported(RenderFeature::RoundedBlit));
                    }
                    if *composite != CompositeMode::SourceOver {
                        return Err(RenderError::Unsupported(RenderFeature::Composite(
                            *composite,
                        )));
                    }
                    if !quad_is_parallelogram(q)
                        || quad_to_affine(q, &Rect::new(0, 0, texture.width, texture.height))
                            .is_none()
                    {
                        return Err(RenderError::Unsupported(RenderFeature::ProjectiveGeometry));
                    }
                }
                if *radius != Fixed::ZERO
                    && request.command.transform().classify() != TransformClass::Identity
                {
                    return Err(RenderError::Unsupported(RenderFeature::RoundedBlit));
                }
                if matches!(
                    texture.format,
                    ColorFormat::RGB565 | ColorFormat::RGB565Swapped
                ) {
                    return Err(RenderError::Unsupported(RenderFeature::TextureFormat(
                        texture.format,
                    )));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

impl Renderer for WebCanvasRenderer<'_> {
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate_projection()?;
        Self::classify_request(request)?;
        if request.projective.is_identity() {
            return Ok(RenderRoute::Native);
        }
        let fallback = self
            .factory
            .projective_fallback
            .as_ref()
            .ok_or(RenderError::MissingWorkspace)?;
        let plan = fallback
            .plan(
                request.command,
                &request.clip,
                &request.projective,
                self.viewport,
            )
            .map_err(RenderError::from)?;
        Ok(RenderRoute::ExactFallback {
            required_bytes: plan.required_bytes(),
        })
    }

    fn output_scale(&self) -> Fixed {
        self.viewport.scale()
    }

    fn supports_offscreen(&self) -> bool {
        true
    }

    fn offscreen_format(&self) -> Option<ColorFormat> {
        Some(ColorFormat::RGBA8888)
    }

    fn sample_target_region(&self, src: &Rect) -> Option<crate::render::texture::Texture<'static>> {
        let phys = self.physical_clip_rect(src)?;
        let img = self
            .ctx()
            .get_image_data(
                phys.x.to_f32() as f64,
                phys.y.to_f32() as f64,
                phys.w.to_f32() as f64,
                phys.h.to_f32() as f64,
            )
            .ok()?;
        let w = phys.w.to_int() as u16;
        let h = phys.h.to_int() as u16;
        let data = img.data();
        let mut tex = crate::render::texture::Texture::owned(w, h, ColorFormat::RGBA8888)
            .with_transient(true);
        if let crate::render::texture::TexBuf::Owned(ref mut dst) = tex.buf {
            dst.copy_from_slice(&data.0);
        }
        Some(tex)
    }

    fn read_target_region(&self, src: &Rect, dst: &mut crate::render::texture::Texture) {
        let Some(phys) = self.physical_clip_rect(src) else {
            return;
        };
        let Ok(img) = self.ctx().get_image_data(
            phys.x.to_f32() as f64,
            phys.y.to_f32() as f64,
            phys.w.to_f32() as f64,
            phys.h.to_f32() as f64,
        ) else {
            return;
        };
        let src_bytes = &img.data().0;
        let dst_bytes = match &mut dst.buf {
            crate::render::texture::TexBuf::Owned(v) => v.as_mut_slice(),
            _ => return,
        };
        let copy_len = src_bytes.len().min(dst_bytes.len());
        dst_bytes[..copy_len].copy_from_slice(&src_bytes[..copy_len]);
    }

    fn modify_target_region(
        &mut self,
        src: &Rect,
        f: &mut dyn FnMut(&mut crate::render::texture::Texture),
    ) -> bool {
        let Some(mut tex) = self.sample_target_region(src) else {
            return false;
        };
        f(&mut tex);
        let Some(phys) = self.physical_clip_rect(src) else {
            return false;
        };
        let bytes = match &tex.buf {
            crate::render::texture::TexBuf::Owned(v) => v.as_slice(),
            _ => return false,
        };
        let mut buf: alloc::vec::Vec<u8> = bytes.into();
        let Ok(img) = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
            wasm_bindgen::Clamped(&mut buf),
            tex.width as u32,
            tex.height as u32,
        ) else {
            return false;
        };
        self.ctx()
            .put_image_data(&img, phys.x.to_f32() as f64, phys.y.to_f32() as f64)
            .is_ok()
    }

    fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        let dpr = self.dpr();

        match cmd {
            DrawCommand::PushClip {
                path,
                transform,
                fill_rule,
            } => {
                let ctx = self.ctx();
                ctx.save();
                let tf = transform;
                ctx.set_transform(
                    tf.m00.to_f32() as f64 * dpr,
                    tf.m10.to_f32() as f64 * dpr,
                    tf.m01.to_f32() as f64 * dpr,
                    tf.m11.to_f32() as f64 * dpr,
                    tf.tx.to_f32() as f64 * dpr,
                    tf.ty.to_f32() as f64 * dpr,
                )
                .unwrap();
                self.build_path(path);
                match fill_rule {
                    crate::render::raster::FillRule::EvenOdd => {
                        ctx.clip_with_canvas_winding_rule(CanvasWindingRule::Evenodd);
                    }
                    crate::render::raster::FillRule::NonZero => {
                        ctx.clip();
                    }
                }
                return;
            }
            DrawCommand::PopClip => {
                self.ctx().restore();
                return;
            }
            DrawCommand::ApplyBlur { alpha, region } => {
                let radius_f = (alpha.to_f32() * 10.0).max(0.0);
                if radius_f > 0.0 {
                    let dpr = self.dpr();
                    let rx = region.x.to_f32() as f64 * dpr;
                    let ry = region.y.to_f32() as f64 * dpr;
                    let rw = region.w.to_f32() as f64 * dpr;
                    let rh = region.h.to_f32() as f64 * dpr;
                    let window = web_sys::window().unwrap();
                    let doc = window.document().unwrap();
                    let off = doc
                        .create_element("canvas")
                        .unwrap()
                        .unchecked_into::<web_sys::HtmlCanvasElement>();
                    off.set_width(rw.ceil() as u32);
                    off.set_height(rh.ceil() as u32);
                    let off_ctx = off
                        .get_context("2d")
                        .unwrap()
                        .unwrap()
                        .unchecked_into::<web_sys::CanvasRenderingContext2d>();
                    off_ctx.set_filter(&alloc::format!("blur({}px)", radius_f as f64 * dpr));
                    let src_canvas = self.surface.canvas();
                    off_ctx
                        .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                            src_canvas, rx, ry, rw, rh, 0.0, 0.0, rw, rh,
                        )
                        .unwrap();
                    let ctx = self.ctx();
                    ctx.set_transform(1.0, 0.0, 0.0, 1.0, 0.0, 0.0).unwrap();
                    ctx.clear_rect(rx, ry, rw, rh);
                    ctx.draw_image_with_html_canvas_element(&off, rx, ry)
                        .unwrap();
                }
                return;
            }
            _ => {}
        }

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
                let paint = Paint::Color((*color).into());
                self.fill_quad_inner(q, area, clip, &paint, *radius, *opa);
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
                let paint = Paint::Color((*color).into());
                self.stroke_quad_inner(q, area, *width, clip, &paint, *radius, *opa);
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
                radius,
                composite,
                ..
            } => {
                let src_rect = Rect::new(0, 0, texture.width, texture.height);
                self.blit(
                    texture, &src_rect, *pos, *size, clip, *opa, *radius, *composite,
                );
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
                path,
                paint,
                opa,
                fill_rule,
                ..
            } => {
                self.fill_path(path, clip, paint, *opa, *fill_rule);
            }
            DrawCommand::StrokePath {
                path,
                width,
                paint,
                opa,
                line_cap,
                line_join,
                miter_limit,
                dash,
                ..
            } => {
                self.stroke_path(
                    path,
                    clip,
                    *width,
                    paint,
                    *opa,
                    *line_cap,
                    *line_join,
                    *miter_limit,
                    dash,
                );
            }
            DrawCommand::GlyphRun {
                pos,
                glyphs,
                font,
                transform,
                color,
                opa,
            } => {
                self.draw_glyph_run_inner(GlyphRunDraw {
                    pos,
                    glyphs,
                    font,
                    transform,
                    clip,
                    color,
                    opacity: *opa,
                });
            }
            DrawCommand::PosedGlyphRun {
                pos,
                glyphs,
                font,
                transform,
                color,
                opa,
            } => {
                self.draw_posed_glyph_run_inner(PosedGlyphRunDraw {
                    pos,
                    glyphs: glyphs.glyphs(),
                    frames: glyphs.frames(),
                    font,
                    transform,
                    clip,
                    color,
                    opacity: *opa,
                });
            }
            DrawCommand::PushClip { .. } | DrawCommand::PopClip | DrawCommand::ApplyBlur { .. } => {
            }
        }

        self.ctx().restore();
    }

    fn draw_projective(
        &mut self,
        command: &DrawCommand,
        clip: &Rect,
        projective: &Transform3D,
    ) -> Result<(), ProjectiveDrawError> {
        if projective.is_identity() {
            self.draw(command, clip);
            return Ok(());
        }
        let plan = self
            .factory
            .projective_fallback
            .as_ref()
            .ok_or(ProjectiveDrawError::MissingFallbackStorage)?
            .plan(command, clip, projective, self.viewport)?;
        if plan.width == 0 || plan.height == 0 {
            return Ok(());
        }

        let image = self
            .ctx()
            .get_image_data(
                f64::from(plan.x),
                f64::from(plan.y),
                f64::from(plan.width),
                f64::from(plan.height),
            )
            .map_err(|_| ProjectiveDrawError::Unsupported)?;
        let source = image.data();
        let fallback = self
            .factory
            .projective_fallback
            .as_mut()
            .ok_or(ProjectiveDrawError::MissingFallbackStorage)?;
        fallback.target_mut(plan).copy_from_slice(&source.0);
        fallback.render(plan, command, projective, self.viewport)?;
        let output = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
            wasm_bindgen::Clamped(fallback.target_mut(plan)),
            u32::from(plan.width),
            u32::from(plan.height),
        )
        .map_err(|_| ProjectiveDrawError::Unsupported)?;
        self.ctx()
            .put_image_data(&output, f64::from(plan.x), f64::from(plan.y))
            .map_err(|_| ProjectiveDrawError::Unsupported)
    }

    fn preflight_projective(
        &self,
        command: &DrawCommand,
        clip: &Rect,
        projective: &Transform3D,
    ) -> Result<(), ProjectiveDrawError> {
        if projective.is_identity() {
            return Ok(());
        }
        let fallback = self
            .factory
            .projective_fallback
            .as_ref()
            .ok_or(ProjectiveDrawError::MissingFallbackStorage)?;
        fallback
            .plan(command, clip, projective, self.viewport)
            .map(|_| ())
    }

    fn flush(&mut self) {
        Canvas::flush(self)
    }
}

impl WebCanvasRenderer<'_> {
    fn draw_posed_glyph_run_inner(&mut self, draw: PosedGlyphRunDraw<'_>) {
        for (positioned, frame) in draw.glyphs.iter().zip(draw.frames) {
            let tangent_x = crate::types::fixed::from_textflow(frame.unit_tangent.x);
            let tangent_y = crate::types::fixed::from_textflow(frame.unit_tangent.y);
            let origin_x = draw.pos.x + crate::types::fixed::from_textflow(frame.local_origin.x);
            let origin_y = draw.pos.y + crate::types::fixed::from_textflow(frame.local_origin.y);
            let ctx = self.ctx();
            ctx.save();
            let transformed = ctx.transform(
                tangent_x.to_f32() as f64,
                tangent_y.to_f32() as f64,
                -tangent_y.to_f32() as f64,
                tangent_x.to_f32() as f64,
                origin_x.to_f32() as f64,
                origin_y.to_f32() as f64,
            );
            if transformed.is_ok() {
                let glyph = [textflow::shaping::PositionedGlyph::new(
                    positioned.glyph_id(),
                    textflow::shaping::FlowPoint { x: 0, y: 0 },
                )];
                self.draw_glyph_run_inner(GlyphRunDraw {
                    pos: &Point::ZERO,
                    glyphs: &glyph,
                    font: draw.font,
                    transform: draw.transform,
                    clip: draw.clip,
                    color: draw.color,
                    opacity: draw.opacity,
                });
            }
            self.ctx().restore();
        }
    }

    fn draw_glyph_run_inner(&mut self, draw: GlyphRunDraw<'_>) {
        let GlyphRunDraw {
            pos,
            glyphs,
            font,
            transform,
            clip,
            color,
            opacity,
        } = draw;
        let scale = self.viewport.scale() * transform.raster_scale();
        let output_ppem = crate::render::font::output_ppem(font.size, scale);
        let Some(bounds) = font.raster_run_bounds(glyphs, output_ppem, scale) else {
            return;
        };
        let key = font.raster_run_key(glyphs, color, scale);
        let pw = bounds.width;
        let ph = bounds.height;
        let handle = match self
            .factory
            .glyph_pool
            .entry(key)
            .or_try_insert_with::<_, ()>(|| {
                let mut buf = alloc::vec![0u8; usize::from(pw) * usize::from(ph) * 4];
                {
                    let mut texture = Texture::new(&mut buf, pw, ph, ColorFormat::RGBA8888);
                    texture.alpha_mode = AlphaMode::Blend;
                    let mut sw = SwRenderer::new(texture);
                    sw.viewport = Viewport::new(pw, ph, scale);
                    let origin = Point {
                        x: -bounds.offset.x,
                        y: -bounds.offset.y,
                    };
                    let full = Rect {
                        x: Fixed::ZERO,
                        y: Fixed::ZERO,
                        w: bounds.size.x,
                        h: bounds.size.y,
                    };
                    let raster_color = Color { a: 255, ..*color };
                    sw.draw_glyph_run(&origin, glyphs, font, &full, &raster_color, 255);
                }
                unpremultiply_rgba(&mut buf);
                let texture = Texture::new(&mut buf, pw, ph, ColorFormat::RGBA8888);
                texture_pool::upload(&texture).ok_or(())
            }) {
            Ok(handle) => handle,
            Err(_) => return,
        };
        self.push_rect_clip(clip);
        self.ctx()
            .set_global_alpha((color.a as f64 * opacity as f64) / (255.0 * 255.0));
        let _ = self
            .ctx()
            .draw_image_with_offscreen_canvas_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                &handle.get().canvas,
                0.0,
                0.0,
                f64::from(pw),
                f64::from(ph),
                (pos.x + bounds.offset.x).to_f32() as f64,
                (pos.y + bounds.offset.y).to_f32() as f64,
                bounds.size.x.to_f32() as f64,
                bounds.size.y.to_f32() as f64,
            );
        self.pop_rect_clip();
    }
}

impl Canvas for WebCanvasRenderer<'_> {
    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        self.push_rect_clip(clip);
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
        self.pop_rect_clip();
    }

    fn fill_path(
        &mut self,
        path: &Path,
        clip: &Rect,
        paint: &Paint,
        opa: u8,
        fill_rule: crate::render::raster::FillRule,
    ) {
        self.push_rect_clip(clip);
        let bbox = path.bbox();
        self.set_paint_style(paint, opa, bbox);
        self.build_path(path);
        match fill_rule {
            crate::render::raster::FillRule::EvenOdd => self
                .ctx()
                .fill_with_canvas_winding_rule(CanvasWindingRule::Evenodd),
            crate::render::raster::FillRule::NonZero => self.ctx().fill(),
        }
        self.pop_rect_clip();
    }

    fn stroke_path(
        &mut self,
        path: &Path,
        clip: &Rect,
        width: Fixed,
        paint: &Paint,
        opa: u8,
        cap: crate::render::raster::LineCap,
        join: crate::render::raster::LineJoin,
        miter_limit: Fixed,
        _dash: &[Fixed],
    ) {
        self.push_rect_clip(clip);
        let bbox = path.bbox();
        self.set_stroke_style(paint, width, opa, cap, join, miter_limit, bbox);
        self.build_path(path);
        self.ctx().stroke();
        self.pop_rect_clip();
    }

    fn blit(
        &mut self,
        src: &Texture,
        src_rect: &Rect,
        dst: Point,
        dst_size: Point,
        clip: &Rect,
        opa: u8,
        radius: Fixed,
        composite: CompositeMode,
    ) {
        if opa == 0 {
            return;
        }
        // Transient textures (sample_target_region results) reuse heap
        // slots, so TextureKey ptr collisions would return a stale
        // cached OffscreenCanvas. Upload fresh, skip the pool.
        let transient_up;
        let pooled_handle;
        let canvas_ref: &web_sys::OffscreenCanvas = if src.transient {
            transient_up = match texture_pool::upload(src) {
                Some(up) => up,
                None => return,
            };
            &transient_up.canvas
        } else {
            let key = TextureKey::from(src);
            pooled_handle = match self
                .factory
                .texture_pool
                .entry(key)
                .or_try_insert_with::<_, ()>(|| texture_pool::upload(src).ok_or(()))
            {
                Ok(h) => h,
                Err(_) => return,
            };
            if pooled_handle.is_invalid() {
                return;
            }
            &pooled_handle.canvas
        };

        self.push_rect_clip(clip);
        let ctx = self.ctx();
        let prev_alpha = ctx.global_alpha();
        let prev_composite = ctx.global_composite_operation().unwrap_or_default();
        ctx.set_global_alpha(opa as f64 / 255.0);
        let op = match composite {
            CompositeMode::SourceOver => "source-over",
            CompositeMode::Add => "lighter",
            CompositeMode::Screen => "screen",
            CompositeMode::Multiply => "multiply",
            CompositeMode::Darken => "darken",
            CompositeMode::Lighten => "lighten",
            CompositeMode::Difference => "difference",
        };
        let _ = ctx.set_global_composite_operation(op);

        if radius > Fixed::ZERO {
            let saved = ctx.get_transform().expect("getTransform");
            let d = self.dpr();
            ctx.set_transform(d, 0.0, 0.0, d, 0.0, 0.0)
                .expect("setTransform(dpr)");
            ctx.save();
            ctx.begin_path();
            let x = dst.x.to_f32() as f64;
            let y = dst.y.to_f32() as f64;
            let w = dst_size.x.to_f32() as f64;
            let h = dst_size.y.to_f32() as f64;
            let r = radius.to_f32() as f64;
            let r = r.min(w / 2.0).min(h / 2.0);
            ctx.move_to(x + r, y);
            let _ = ctx.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
            let _ = ctx.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
            let _ = ctx.arc(
                x + r,
                y + h - r,
                r,
                std::f64::consts::FRAC_PI_2,
                std::f64::consts::PI,
            );
            let _ = ctx.arc(
                x + r,
                y + r,
                r,
                std::f64::consts::PI,
                std::f64::consts::FRAC_PI_2 * 3.0,
            );
            ctx.close_path();
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

        let result = ctx
            .draw_image_with_offscreen_canvas_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                canvas_ref,
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

        if radius > Fixed::ZERO {
            ctx.restore();
        }

        ctx.set_global_alpha(prev_alpha);
        let _ = ctx.set_global_composite_operation(&prev_composite);
        self.pop_rect_clip();
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

    fn draw_glyph_run(
        &mut self,
        pos: &Point,
        glyphs: &[textflow::shaping::PositionedGlyph],
        font: &crate::render::font::Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        self.draw_glyph_run_inner(GlyphRunDraw {
            pos,
            glyphs,
            font,
            transform: &Transform::IDENTITY,
            clip,
            color,
            opacity: opa,
        });
    }

    fn draw_posed_glyph_run(
        &mut self,
        pos: &Point,
        glyphs: PosedGlyphs<'_>,
        font: &crate::render::font::Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        self.draw_posed_glyph_run_inner(PosedGlyphRunDraw {
            pos,
            glyphs: glyphs.glyphs(),
            frames: glyphs.frames(),
            font,
            transform: &Transform::IDENTITY,
            clip,
            color,
            opacity: opa,
        });
    }

    fn push_clip(
        &mut self,
        path: &Path,
        _transform: &Transform,
        fill_rule: crate::render::raster::FillRule,
    ) {
        self.ctx().save();
        self.build_path(path);
        match fill_rule {
            crate::render::raster::FillRule::EvenOdd => self
                .ctx()
                .clip_with_canvas_winding_rule(CanvasWindingRule::Evenodd),
            crate::render::raster::FillRule::NonZero => self.ctx().clip(),
        }
    }

    fn pop_clip(&mut self) {
        self.ctx().restore();
    }

    fn flush(&mut self) {}
}

fn css_color(c: &Color) -> String {
    format!("rgb({}, {}, {})", c.r, c.g, c.b)
}

fn unpremultiply_rgba(bytes: &mut [u8]) {
    for pixel in bytes.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        if alpha != 0 && alpha != 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
}

fn css_color_with_opa(c: impl Into<Color>, opa: u8) -> String {
    let color = c.into().scale_alpha(opa);
    format!(
        "rgba({}, {}, {}, {:.6})",
        color.r,
        color.g,
        color.b,
        color.a as f64 / 255.0
    )
}

fn add_gradient_stops(gradient: &CanvasGradient, stops: &[mirx::scene::GradientStop], opa: u8) {
    for stop in stops {
        let offset: Fixed = stop.offset.into();
        let _ = gradient.add_color_stop(
            offset.to_f32().clamp(0.0, 1.0),
            &css_color_with_opa(stop.color, opa),
        );
    }
}

fn line_cap_str(cap: LineCap) -> &'static str {
    match cap {
        LineCap::Butt => "butt",
        LineCap::Round => "round",
        LineCap::Square => "square",
    }
}

fn line_join_str(join: LineJoin) -> &'static str {
    match join {
        LineJoin::Miter => "miter",
        LineJoin::Round => "round",
        LineJoin::Bevel => "bevel",
    }
}

fn quad_is_parallelogram(q: &[Point; 4]) -> bool {
    let x = |index: usize| q[index].x.to_f32() as f64;
    let y = |index: usize| q[index].y.to_f32() as f64;
    x(0) + x(2) == x(1) + x(3) && y(0) + y(2) == y(1) + y(3)
}

/// Returns the affine transform mapping `area` to `q` when its opposite
/// edges agree within the rounding tolerance.
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
