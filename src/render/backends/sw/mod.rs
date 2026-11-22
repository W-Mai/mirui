use crate::types::{Color, Fixed, Point, Rect, Transform, Transform3D, Viewport};

use crate::render::canvas::{Canvas, Paint};
use crate::render::command::{CompositeMode, DrawCommand};
use crate::render::path::{Path, PathCmd};
use crate::render::renderer::{
    DrawRequest, FallbackRegion, ProjectiveDrawError, RenderError, RenderFeature, RenderRoute,
    Renderer,
};
use crate::render::texture::{Texture, TexturePixels, TexturePixelsMut};

#[cfg(feature = "perf")]
pub mod perf;
#[cfg(feature = "perf")]
pub use perf::{PerfCtx, quad_perf};

mod blit_dispatch;
mod blit_fast;
mod blit_fractional;
mod blit_rgba;
pub mod blur;
mod label;
mod label_sdf;
mod line;
pub mod mix;
mod path;
mod prepared;
mod quad;
mod quad_aa;
mod rect_fill;
mod rect_stroke;
mod transformed;
use quad::{blit_quad, fill_rect_quad, stroke_rect_quad};
use transformed::{blit_transformed, fill_rect_transformed, offset_point, offset_rect};

pub use crate::render::texture::AlphaMode;

pub struct SwRenderer<'a> {
    pub target: Texture<'a>,
    pub viewport: Viewport,
    pub(super) scratch: ScratchStorage<'a>,
    failure: Option<RenderError>,
    physical_clip: Option<crate::types::PhysicalRect>,
    #[cfg(feature = "perf")]
    pub perf: Option<PerfCtx>,
}

fn write_pixels<'a>(
    target: &'a mut Texture<'_>,
    failure: &mut Option<RenderError>,
) -> Option<TexturePixelsMut<'a>> {
    if failure.is_some() {
        return None;
    }
    match target.pixels_mut() {
        Ok(pixels) => Some(pixels),
        Err(error) => {
            *failure = Some(error);
            None
        }
    }
}

pub(crate) struct SwScratch {
    pub(super) flatten_buf: alloc::vec::Vec<crate::render::raster::LineSeg>,
    pub(super) primitive_path: crate::render::path::Path,
    pub(super) stroke_outline: crate::render::path::Path,
    pub(super) subpath_scratch: alloc::vec::Vec<crate::render::raster::SubPath>,
    pub(super) semantic_joins: alloc::vec::Vec<usize>,
    pub(super) dash_segments: alloc::vec::Vec<crate::render::raster::LineSeg>,
    pub(super) dash_scratch: alloc::vec::Vec<crate::render::raster::SubPath>,
    pub(super) dashed_semantic_joins: alloc::vec::Vec<usize>,
    pub(super) scanline_acc: alloc::vec::Vec<Fixed>,
    pub(super) scanline_crossings: alloc::vec::Vec<(Fixed, i8)>,
    pub(super) stroke_normals: alloc::vec::Vec<crate::types::Point>,
    pub(super) stroke_rail: alloc::vec::Vec<crate::types::Point>,
    pub(super) stroke_left_rail: alloc::vec::Vec<crate::types::Point>,
    pub(super) stroke_arc: alloc::vec::Vec<crate::types::Point>,
    pub(super) clip_stack: alloc::vec::Vec<ClipMask>,
    pub(super) clip_recycled: alloc::vec::Vec<ClipMask>,
    pub(super) clip_mask_buf: alloc::vec::Vec<u8>,
}

#[allow(
    clippy::large_enum_variant,
    reason = "direct renderer scratch stays on the stack"
)]
pub(super) enum ScratchStorage<'a> {
    Owned(SwScratch),
    Borrowed(&'a mut SwScratch),
}

impl core::ops::Deref for ScratchStorage<'_> {
    type Target = SwScratch;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(scratch) => scratch,
            Self::Borrowed(scratch) => scratch,
        }
    }
}

impl core::ops::DerefMut for ScratchStorage<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Owned(scratch) => scratch,
            Self::Borrowed(scratch) => scratch,
        }
    }
}

pub(super) struct ClipMask {
    pub alpha: alloc::vec::Vec<u8>,
}

impl SwScratch {
    pub(crate) fn new() -> Self {
        Self {
            flatten_buf: alloc::vec::Vec::new(),
            primitive_path: crate::render::path::Path::new(),
            stroke_outline: crate::render::path::Path::new(),
            subpath_scratch: alloc::vec::Vec::new(),
            semantic_joins: alloc::vec::Vec::new(),
            dash_segments: alloc::vec::Vec::new(),
            dash_scratch: alloc::vec::Vec::new(),
            dashed_semantic_joins: alloc::vec::Vec::new(),
            scanline_acc: alloc::vec::Vec::new(),
            scanline_crossings: alloc::vec::Vec::new(),
            stroke_normals: alloc::vec::Vec::new(),
            stroke_rail: alloc::vec::Vec::new(),
            stroke_left_rail: alloc::vec::Vec::new(),
            stroke_arc: alloc::vec::Vec::new(),
            clip_stack: alloc::vec::Vec::new(),
            clip_recycled: alloc::vec::Vec::new(),
            clip_mask_buf: alloc::vec::Vec::new(),
        }
    }

    pub(crate) fn prepare_scanline(&mut self, width: usize) {
        if self.scanline_acc.capacity() < width {
            self.scanline_acc
                .reserve(width.saturating_sub(self.scanline_acc.len()));
        }
    }

    fn reset_frame(&mut self) {
        while let Some(mut mask) = self.clip_stack.pop() {
            mask.alpha.clear();
            self.clip_recycled.push(mask);
        }
    }

    #[cfg(test)]
    pub(crate) fn retained_buffer_state(&self) -> (usize, usize, usize, usize) {
        assert!(self.flatten_buf.capacity() > 0);
        assert!(self.scanline_acc.capacity() > 0);
        assert!(self.stroke_rail.capacity() > 0);
        let mask = &self.clip_recycled[0].alpha;
        assert!(mask.capacity() >= 64 * 64);
        (
            self.flatten_buf.as_ptr() as usize,
            self.scanline_acc.as_ptr() as usize,
            self.stroke_rail.as_ptr() as usize,
            mask.as_ptr() as usize,
        )
    }

    #[cfg(test)]
    pub(crate) fn has_no_retained_buffers(&self) -> bool {
        self.flatten_buf.capacity() == 0
            && self.scanline_acc.capacity() == 0
            && self.stroke_rail.capacity() == 0
            && self.clip_recycled.capacity() == 0
            && self.clip_mask_buf.capacity() == 0
    }
}

impl<'a> SwRenderer<'a> {
    pub fn new(target: Texture<'a>) -> Self {
        Self::with_storage(target, ScratchStorage::Owned(SwScratch::new()))
    }

    pub(crate) fn with_scratch(target: Texture<'a>, scratch: &'a mut SwScratch) -> Self {
        scratch.reset_frame();
        Self::with_storage(target, ScratchStorage::Borrowed(scratch))
    }

    fn physical_draw_clip(&self, clip: Rect) -> Rect {
        crate::render::renderer::constrain_physical_clip(
            self.viewport.rect_to_physical(clip),
            self.physical_clip,
        )
    }

    fn with_storage(target: Texture<'a>, scratch: ScratchStorage<'a>) -> Self {
        let viewport = Viewport::new(target.width, target.height, Fixed::ONE);
        Self {
            target,
            viewport,
            scratch,
            failure: None,
            physical_clip: None,
            #[cfg(feature = "perf")]
            perf: None,
        }
    }

    /// Sets the destination buffer's alpha mode. `Opaque` (the
    /// default) writes `dst.a = 255` on every pixel — correct for
    /// framebuffer output. `Blend` accumulates `dst.a` via
    /// non-premultiplied source-over so a sampler reading the
    /// buffer's alpha channel sees a correct silhouette; intended
    /// for offscreen buffers feeding effect widgets that read the
    /// alpha channel.
    pub fn with_alpha_mode(mut self, mode: AlphaMode) -> Self {
        self.target.alpha_mode = mode;
        self
    }

    fn source_pixels<'s>(&mut self, source: &'s Texture<'_>) -> Option<TexturePixels<'s>> {
        if self.failure.is_some() {
            return None;
        }
        match source.read_pixels() {
            Ok(pixels) => Some(pixels),
            Err(error) => {
                self.failure = Some(error);
                None
            }
        }
    }
}

impl<'a> SwRenderer<'a> {
    /// Each `DrawCommand` arm is `#[inline(never)]` so native submission
    /// stays small enough to fit ESP32-C3's 16 KiB ICache. Letting LLVM
    /// inline the full dispatch chain produced a 20 KiB monolith that
    /// guaranteed cache miss every frame.
    #[inline(never)]
    fn draw_transformed(&mut self, cmd: &DrawCommand, clip: &Rect, tf: &Transform) {
        let vp = self.viewport.as_transform();
        let phys_tf = vp.compose(tf);
        let phys_clip = self.physical_draw_clip(*clip);
        match cmd {
            DrawCommand::PushClip {
                path, fill_rule, ..
            } => {
                self.push_clip_inner(path, &phys_tf, *fill_rule);
            }
            DrawCommand::PopClip => {
                self.pop_clip();
            }
            DrawCommand::ApplyBlur { alpha, region } => {
                let phys_region = self.physical_draw_clip(*region);
                let Some(mut target) = write_pixels(&mut self.target, &mut self.failure) else {
                    return;
                };
                crate::render::backends::sw::blur::iir_blur_inplace(
                    &mut target,
                    *alpha,
                    phys_region,
                );
            }
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
                ..
            } => {
                if *radius != Fixed::ZERO {
                    self.dispatch_fill_quad(
                        &tf.apply_rect(*area),
                        area,
                        color,
                        *radius,
                        *opa,
                        clip,
                    );
                    return;
                }
                let Some(mut target) = write_pixels(&mut self.target, &mut self.failure) else {
                    return;
                };
                fill_rect_transformed(&mut target, *area, phys_clip, &phys_tf, color, *opa);
            }
            DrawCommand::Blit {
                pos,
                size,
                texture,
                radius,
                composite,
                ..
            } => {
                if *radius != Fixed::ZERO {
                    unimplemented!(
                        "sw backend: Blit.radius mask under non-axis-aligned transform not yet implemented",
                    );
                }
                if !matches!(composite, CompositeMode::SourceOver) {
                    unimplemented!(
                        "sw backend: composite {composite:?} under non-axis-aligned transform not yet implemented",
                    );
                }
                let Some(pixels) = self.source_pixels(texture) else {
                    return;
                };
                let src_rect = Rect::new(0, 0, pixels.width(), pixels.height());
                let dst = Rect {
                    x: pos.x,
                    y: pos.y,
                    w: size.x,
                    h: size.y,
                };
                let Some(mut target) = write_pixels(&mut self.target, &mut self.failure) else {
                    return;
                };
                blit_transformed(&mut target, &pixels, &src_rect, dst, phys_clip, &phys_tf);
            }
            DrawCommand::FillPath {
                path,
                paint,
                opa,
                fill_rule,
                ..
            } => {
                self.fill_path_transformed(path, phys_clip, &phys_tf, paint, *opa, *fill_rule);
            }
            DrawCommand::StrokePath {
                path,
                paint,
                width,
                opa,
                line_cap,
                line_join,
                miter_limit,
                dash,
                ..
            } => {
                self.stroke_path_transformed(
                    path,
                    phys_clip,
                    &phys_tf,
                    *width,
                    paint,
                    *opa,
                    *line_cap,
                    *line_join,
                    *miter_limit,
                    dash,
                );
            }
            DrawCommand::Border {
                area,
                color,
                width,
                radius,
                opa,
                ..
            } => {
                let path = crate::render::path::Path::rounded_rect(
                    area.x + *width / 2,
                    area.y + *width / 2,
                    area.w - *width,
                    area.h - *width,
                    (*radius - *width / 2).max(Fixed::ZERO),
                );
                let paint = Paint::Color((*color).into());
                self.stroke_path_transformed(
                    &path,
                    phys_clip,
                    &phys_tf,
                    *width,
                    &paint,
                    *opa,
                    crate::render::raster::LineCap::Butt,
                    crate::render::raster::LineJoin::Miter,
                    Fixed::from_int(4),
                    &[],
                );
            }
            DrawCommand::Line {
                p1,
                p2,
                color,
                width,
                opa,
                ..
            } => {
                let mut path = crate::render::path::Path::new();
                path.move_to(*p1).line_to(*p2);
                let paint = Paint::Color((*color).into());
                self.stroke_path_transformed(
                    &path,
                    phys_clip,
                    &phys_tf,
                    *width,
                    &paint,
                    *opa,
                    crate::render::raster::LineCap::Butt,
                    crate::render::raster::LineJoin::Miter,
                    Fixed::from_int(4),
                    &[],
                );
            }
            DrawCommand::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
                ..
            } => {
                let mut path = core::mem::take(&mut self.scratch.primitive_path);
                path.set_arc(*center, *radius, *start_angle, *end_angle);
                let paint = Paint::Color((*color).into());
                self.stroke_path_transformed(
                    &path,
                    phys_clip,
                    &phys_tf,
                    *width,
                    &paint,
                    *opa,
                    crate::render::raster::LineCap::Butt,
                    crate::render::raster::LineJoin::Miter,
                    Fixed::from_int(4),
                    &[],
                );
                self.scratch.primitive_path = path;
            }
            DrawCommand::GlyphRun {
                pos,
                glyphs,
                font,
                color,
                opa,
                ..
            } => self.draw_glyph_run_transformed_inner(label::TransformedRun {
                pos,
                glyphs,
                font,
                transform: &phys_tf,
                clip: phys_clip,
                color,
                opacity: *opa,
            }),
            DrawCommand::PosedGlyphRun {
                pos,
                glyphs,
                font,
                color,
                opa,
                ..
            } => self.draw_posed_glyph_run_inner(label::PosedRun {
                pos,
                glyphs: glyphs.glyphs(),
                frames: glyphs.frames(),
                font,
                transform: &phys_tf,
                clip: phys_clip,
                color,
                opacity: *opa,
            }),
        }
    }
}

impl<'a> Canvas for SwRenderer<'a> {
    fn fill_path(
        &mut self,
        path: &Path,
        clip: &Rect,
        paint: &Paint,
        opa: u8,
        fill_rule: crate::render::raster::FillRule,
    ) {
        self.fill_path_inner(path, clip, paint, opa, fill_rule);
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
        dash: &[Fixed],
    ) {
        self.stroke_path_inner(path, clip, width, paint, opa, cap, join, miter_limit, dash);
    }

    fn draw_line(
        &mut self,
        p1: Point,
        p2: Point,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opa: u8,
    ) {
        if self.try_aligned_line(p1, p2, clip, width, color, opa) {
            return;
        }
        let commands = [PathCmd::MoveTo(p1), PathCmd::LineTo(p2)];
        let paint = Paint::Color((*color).into());
        self.stroke_commands_inner(
            &commands,
            clip,
            width,
            &paint,
            opa,
            crate::render::raster::LineCap::Butt,
            crate::render::raster::LineJoin::Miter,
            Fixed::from_int(4),
            &[],
        );
    }

    fn draw_arc(
        &mut self,
        center: Point,
        radius: Fixed,
        start_angle: Fixed,
        end_angle: Fixed,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        opa: u8,
    ) {
        let mut path = core::mem::take(&mut self.scratch.primitive_path);
        path.set_arc(center, radius, start_angle, end_angle);
        let paint = Paint::Color((*color).into());
        self.stroke_commands_inner(
            path.commands(),
            clip,
            width,
            &paint,
            opa,
            crate::render::raster::LineCap::Butt,
            crate::render::raster::LineJoin::Miter,
            Fixed::from_int(4),
            &[],
        );
        self.scratch.primitive_path = path;
    }

    fn fill_rect(&mut self, area: &Rect, clip: &Rect, color: &Color, radius: Fixed, opa: u8) {
        self.fill_rect_inner(area, clip, color, radius, opa);
    }

    fn push_clip(
        &mut self,
        path: &Path,
        transform: &Transform,
        fill_rule: crate::render::raster::FillRule,
    ) {
        let phys_tf = self.viewport.as_transform().compose(transform);
        self.push_clip_inner(path, &phys_tf, fill_rule);
    }

    fn pop_clip(&mut self) {
        if let Some(mut mask) = self.scratch.clip_stack.pop() {
            mask.alpha.clear();
            self.scratch.clip_recycled.push(mask);
        }
    }

    fn stroke_rect(
        &mut self,
        area: &Rect,
        clip: &Rect,
        width: Fixed,
        color: &Color,
        radius: Fixed,
        opa: u8,
    ) {
        self.stroke_rect_inner(area, clip, width, color, radius, opa);
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
        self.blit_inner(src, src_rect, dst, dst_size, clip, opa, radius, composite);
    }

    fn clear(&mut self, area: &crate::types::PhysicalRect, color: &Color) {
        let screen = crate::types::PhysicalRect::from_size(self.target.width, self.target.height);
        let Some(draw_area) = area.intersection(screen) else {
            return;
        };
        if !self.scratch.clip_stack.is_empty() {
            self.failure
                .get_or_insert(crate::render::RenderError::InvalidScope);
            return;
        }
        let Some(mut target) = write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        rect_fill::overwrite_rect(
            &mut target,
            i32::from(draw_area.x()),
            i32::from(draw_area.y()),
            i32::from(draw_area.right()),
            i32::from(draw_area.bottom()),
            color,
        );
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
        self.draw_glyph_run_inner(pos, glyphs, font, clip, color, opa);
    }

    fn draw_posed_glyph_run(
        &mut self,
        pos: &Point,
        glyphs: crate::render::PosedGlyphs<'_>,
        font: &crate::render::font::Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        self.draw_posed_glyph_run_inner(label::PosedRun {
            pos,
            glyphs: glyphs.glyphs(),
            frames: glyphs.frames(),
            font,
            transform: &Transform::IDENTITY,
            clip: *clip,
            color,
            opacity: opa,
        });
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        self.failure.map_or(Ok(()), Err)
    }
}

impl SwRenderer<'_> {
    // Keep complex admission checks out of the plain submission body.
    #[inline(never)]
    fn dispatch_fill_quad(
        &mut self,
        q: &[Point; 4],
        area: &Rect,
        color: &Color,
        radius: Fixed,
        opa: u8,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = quad_perf::now();
        let phys_clip = self.physical_draw_clip(*clip);
        let phys_q = [
            self.viewport.point_to_physical(q[0]),
            self.viewport.point_to_physical(q[1]),
            self.viewport.point_to_physical(q[2]),
            self.viewport.point_to_physical(q[3]),
        ];
        let s = self.viewport.scale();
        let clip_mask = self.scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let Some(mut target) = write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        fill_rect_quad(
            &mut target,
            &phys_q,
            phys_clip,
            color,
            radius * s,
            area.w * s,
            area.h * s,
            opa,
            clip_mask,
        );
        #[cfg(feature = "perf")]
        quad_perf::add_fill(quad_perf::now().wrapping_sub(t0));
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn dispatch_blit_quad(
        &mut self,
        q: &[Point; 4],
        texture: &Texture,
        size: Point,
        clip: &Rect,
        radius: Fixed,
        opa: u8,
        composite: CompositeMode,
    ) {
        let Some(pixels) = self.source_pixels(texture) else {
            return;
        };
        self.dispatch_blit_quad_pixels(q, &pixels, size, clip, radius, opa, composite);
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_blit_quad_pixels(
        &mut self,
        q: &[Point; 4],
        pixels: &TexturePixels<'_>,
        size: Point,
        clip: &Rect,
        radius: Fixed,
        opa: u8,
        composite: CompositeMode,
    ) {
        #[cfg(feature = "perf")]
        let t0 = quad_perf::now();
        let phys_clip = self.physical_draw_clip(*clip);
        let phys_q = [
            self.viewport.point_to_physical(q[0]),
            self.viewport.point_to_physical(q[1]),
            self.viewport.point_to_physical(q[2]),
            self.viewport.point_to_physical(q[3]),
        ];
        let scale = self.viewport.scale();
        let clip_mask = self.scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let Some(mut target) = write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        blit_quad(
            &mut target,
            pixels,
            &phys_q,
            phys_clip,
            Point {
                x: size.x * scale,
                y: size.y * scale,
            },
            radius * scale,
            opa,
            composite,
            clip_mask,
        );
        #[cfg(feature = "perf")]
        quad_perf::add_blit(quad_perf::now().wrapping_sub(t0));
    }

    #[inline(never)]
    fn dispatch_border_quad(
        &mut self,
        q: &[Point; 4],
        color: &Color,
        width: Fixed,
        radius: Fixed,
        opa: u8,
        clip: &Rect,
    ) {
        let phys_clip = self.physical_draw_clip(*clip);
        let phys_q = [
            self.viewport.point_to_physical(q[0]),
            self.viewport.point_to_physical(q[1]),
            self.viewport.point_to_physical(q[2]),
            self.viewport.point_to_physical(q[3]),
        ];
        let s = self.viewport.scale();
        let clip_mask = self.scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let Some(mut target) = write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        stroke_rect_quad(
            &mut target,
            &phys_q,
            phys_clip,
            color,
            width * s,
            radius * s,
            opa,
            clip_mask,
        );
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn dispatch_fill(
        &mut self,
        area: &Rect,
        color: &Color,
        radius: Fixed,
        opa: u8,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let area = offset_rect(area, tx, ty);
        self.fill_rect(&area, clip, color, radius, opa);
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.fill += (p.clock)() - t0;
            p.count_fill += 1;
        }
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn dispatch_border(
        &mut self,
        area: &Rect,
        color: &Color,
        width: Fixed,
        radius: Fixed,
        opa: u8,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let area = offset_rect(area, tx, ty);
        self.stroke_rect(&area, clip, width, color, radius, opa);
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.stroke += (p.clock)() - t0;
            p.count_stroke += 1;
        }
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn dispatch_blit(
        &mut self,
        pos: &Point,
        size: Point,
        texture: &Texture,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
        opa: u8,
        radius: Fixed,
        composite: CompositeMode,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let src_rect = Rect::new(0, 0, texture.width, texture.height);
        let pos = offset_point(pos, tx, ty);
        self.blit(texture, &src_rect, pos, size, clip, opa, radius, composite);
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.blit += (p.clock)() - t0;
            p.count_blit += 1;
        }
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn dispatch_glyph_run(
        &mut self,
        pos: &Point,
        glyphs: &[textflow::shaping::PositionedGlyph],
        font: &crate::render::font::Font,
        color: &Color,
        opa: u8,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let pos = offset_point(pos, tx, ty);
        self.draw_glyph_run(&pos, glyphs, font, clip, color, opa);
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.label += (p.clock)() - t0;
            p.count_label += 1;
        }
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn dispatch_line(
        &mut self,
        p1: &Point,
        p2: &Point,
        color: &Color,
        width: Fixed,
        opa: u8,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let p1 = offset_point(p1, tx, ty);
        let p2 = offset_point(p2, tx, ty);
        self.draw_line(p1, p2, clip, width, color, opa);
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.stroke += (p.clock)() - t0;
            p.count_stroke += 1;
        }
    }

    #[inline(never)]
    #[allow(clippy::too_many_arguments)]
    fn dispatch_arc(
        &mut self,
        center: &Point,
        radius: Fixed,
        start_angle: Fixed,
        end_angle: Fixed,
        color: &Color,
        width: Fixed,
        opa: u8,
        tx: Fixed,
        ty: Fixed,
        clip: &Rect,
    ) {
        #[cfg(feature = "perf")]
        let t0 = self.perf.as_ref().map(|p| (p.clock)());
        let center = offset_point(center, tx, ty);
        self.draw_arc(
            center,
            radius,
            start_angle,
            end_angle,
            clip,
            width,
            color,
            opa,
        );
        #[cfg(feature = "perf")]
        if let (Some(t0), Some(p)) = (t0, self.perf.as_mut()) {
            p.stroke += (p.clock)() - t0;
            p.count_stroke += 1;
        }
    }
}

impl SwRenderer<'_> {
    #[inline(never)]
    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        request.validate_projection()?;
        request.validate_texture()?;
        request.validate_write_boundary(self.viewport)?;
        if !request.projective.is_identity() {
            self.preflight_projective(request.command, &request.clip, &request.projective)
                .map_err(RenderError::from)?;
            return Ok(RenderRoute::Native);
        }
        self.validate_affine_draw(request.command)?;
        Ok(RenderRoute::Native)
    }

    fn validate_affine_draw(&self, command: &DrawCommand<'_>) -> Result<(), RenderError> {
        use crate::types::TransformClass;

        match command {
            DrawCommand::FillPath {
                path,
                paint: paint @ (Paint::LinearGradient(_) | Paint::RadialGradient(_)),
                transform,
                ..
            } => {
                let Some(bbox) = path.bbox() else {
                    return Err(RenderError::InvalidGeometry);
                };
                let draw = self.viewport.as_transform().compose(transform);
                if crate::render::paint::GradientPaint::new(paint, draw, bbox).is_none() {
                    return Err(RenderError::InvalidGeometry);
                }
            }
            DrawCommand::StrokePath {
                path,
                paint: paint @ (Paint::LinearGradient(_) | Paint::RadialGradient(_)),
                transform,
                width,
                ..
            } => {
                let Some(bbox) = path.bbox() else {
                    return Err(RenderError::InvalidGeometry);
                };
                let half = *width / 2;
                let bbox = Rect::new(
                    bbox.x - half,
                    bbox.y - half,
                    bbox.w + *width,
                    bbox.h + *width,
                );
                let draw = self.viewport.as_transform().compose(transform);
                if crate::render::paint::GradientPaint::new(paint, draw, bbox).is_none() {
                    return Err(RenderError::InvalidGeometry);
                }
            }
            DrawCommand::Blit {
                quad,
                radius,
                composite,
                opa,
                transform,
                ..
            } if quad.is_none()
                && !matches!(
                    transform.classify(),
                    TransformClass::Identity | TransformClass::Translate
                ) =>
            {
                if *radius != Fixed::ZERO {
                    return Err(RenderError::Unsupported(RenderFeature::RoundedBlit));
                }
                if *composite != CompositeMode::SourceOver {
                    return Err(RenderError::Unsupported(RenderFeature::Composite(
                        *composite,
                    )));
                }
                if *opa != 255 {
                    return Err(RenderError::Unsupported(RenderFeature::BlitOpacity));
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn submit_raw(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        let saved = self.physical_clip;
        request.validate_write_boundary(self.viewport)?;
        self.physical_clip = request.physical_clip;
        let result = self.submit_clipped(request);
        self.physical_clip = saved;
        result
    }

    fn submit_clipped(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        self.flush()?;
        request.validate_projection()?;
        request.validate_texture()?;
        if !request.projective.is_identity() {
            return self.draw_projective(request.command, &request.clip, &request.projective);
        }
        self.validate_affine_draw(request.command)?;
        self.draw(request.command, &request.clip);
        self.flush()
    }

    fn output_scale(&self) -> Fixed {
        self.viewport.scale()
    }

    fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
        FallbackRegion::from_logical_bounds(*bounds, self.viewport, Some(usize::MAX))
    }

    pub(crate) fn draw(&mut self, cmd: &DrawCommand, clip: &Rect) {
        use crate::types::TransformClass;

        if self.failure.is_some() {
            return;
        }
        // Quad fast paths short-circuit before the translate/transform branch.
        if let DrawCommand::Fill {
            area,
            quad: Some(q),
            color,
            opa,
            radius,
            ..
        } = cmd
        {
            crate::trace_span!("sw.fill_quad");
            self.dispatch_fill_quad(q, area, color, *radius, *opa, clip);
            return;
        }
        if let DrawCommand::Blit {
            quad: Some(q),
            texture,
            size,
            radius,
            opa,
            composite,
            ..
        } = cmd
        {
            crate::trace_span!("sw.blit_quad");
            self.dispatch_blit_quad(q, texture, *size, clip, *radius, *opa, *composite);
            return;
        }
        if let DrawCommand::Border {
            quad: Some(q),
            color,
            width,
            radius,
            opa,
            ..
        } = cmd
        {
            crate::trace_span!("sw.border_quad");
            self.dispatch_border_quad(q, color, *width, *radius, *opa, clip);
            return;
        }

        let tf = cmd.transform();
        let class = tf.classify();
        if !matches!(class, TransformClass::Identity | TransformClass::Translate) {
            crate::trace_span!("sw.transformed");
            self.draw_transformed(cmd, clip, &tf);
            return;
        }
        let (tx, ty) = match class {
            TransformClass::Identity => (Fixed::ZERO, Fixed::ZERO),
            TransformClass::Translate => (tf.tx, tf.ty),
            _ => unreachable!(),
        };
        match cmd {
            DrawCommand::PushClip {
                path,
                transform,
                fill_rule,
            } => {
                crate::trace_span!("sw.push_clip");
                let phys_tf = self.viewport.as_transform().compose(transform);
                self.push_clip_inner(path, &phys_tf, *fill_rule);
            }
            DrawCommand::PopClip => {
                crate::trace_span!("sw.pop_clip");
                self.pop_clip();
            }
            DrawCommand::ApplyBlur { alpha, region } => {
                crate::trace_span!("sw.apply_blur");
                let phys_region = self.viewport.rect_to_physical(*region);
                let Some(mut target) = write_pixels(&mut self.target, &mut self.failure) else {
                    return;
                };
                crate::render::backends::sw::blur::iir_blur_inplace(
                    &mut target,
                    *alpha,
                    phys_region,
                );
            }
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
                ..
            } => {
                crate::trace_span!("sw.fill");
                self.dispatch_fill(area, color, *radius, *opa, tx, ty, clip);
            }
            DrawCommand::Border {
                area,
                color,
                width,
                radius,
                opa,
                ..
            } => {
                crate::trace_span!("sw.border");
                self.dispatch_border(area, color, *width, *radius, *opa, tx, ty, clip);
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
                crate::trace_span!("sw.blit");
                self.dispatch_blit(pos, *size, texture, tx, ty, clip, *opa, *radius, *composite);
            }
            DrawCommand::GlyphRun {
                pos,
                glyphs,
                font,
                color,
                opa,
                ..
            } => {
                crate::trace_span!("sw.glyph_run");
                self.dispatch_glyph_run(pos, glyphs, font, color, *opa, tx, ty, clip);
            }
            DrawCommand::PosedGlyphRun {
                pos,
                glyphs,
                font,
                color,
                opa,
                transform,
            } => {
                crate::trace_span!("sw.posed_glyph_run");
                let phys_tf = self.viewport.as_transform().compose(transform);
                self.draw_posed_glyph_run_inner(label::PosedRun {
                    pos,
                    glyphs: glyphs.glyphs(),
                    frames: glyphs.frames(),
                    font,
                    transform: &phys_tf,
                    clip: self.physical_draw_clip(*clip),
                    color,
                    opacity: *opa,
                });
            }
            DrawCommand::Line {
                p1,
                p2,
                color,
                width,
                opa,
                ..
            } => {
                crate::trace_span!("sw.line");
                self.dispatch_line(p1, p2, color, *width, *opa, tx, ty, clip);
            }
            DrawCommand::Arc {
                center,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
                ..
            } => {
                crate::trace_span!("sw.arc");
                self.dispatch_arc(
                    center,
                    *radius,
                    *start_angle,
                    *end_angle,
                    color,
                    *width,
                    *opa,
                    tx,
                    ty,
                    clip,
                );
            }
            DrawCommand::FillPath {
                path,
                paint,
                opa,
                fill_rule,
                ..
            } => {
                crate::trace_span!("sw.fill_path");
                if tx == Fixed::ZERO && ty == Fixed::ZERO {
                    self.fill_path_inner(path, clip, paint, *opa, *fill_rule);
                } else {
                    let phys_tf = self
                        .viewport
                        .as_transform()
                        .compose(&Transform::translate(tx, ty));
                    let phys_clip = self.physical_draw_clip(*clip);
                    self.fill_path_transformed(path, phys_clip, &phys_tf, paint, *opa, *fill_rule);
                }
            }
            DrawCommand::StrokePath {
                path,
                paint,
                width,
                opa,
                line_cap,
                line_join,
                miter_limit,
                dash,
                ..
            } => {
                crate::trace_span!("sw.stroke_path");
                if tx == Fixed::ZERO && ty == Fixed::ZERO {
                    self.stroke_path_inner(
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
                } else {
                    let phys_tf = self
                        .viewport
                        .as_transform()
                        .compose(&Transform::translate(tx, ty));
                    let phys_clip = self.physical_draw_clip(*clip);
                    self.stroke_path_transformed(
                        path,
                        phys_clip,
                        &phys_tf,
                        *width,
                        paint,
                        *opa,
                        *line_cap,
                        *line_join,
                        *miter_limit,
                        dash,
                    );
                }
            }
        }
    }

    // Projective drawing has its own stack workspace and dispatch table.
    #[inline(never)]
    pub(crate) fn draw_projective(
        &mut self,
        cmd: &DrawCommand,
        clip: &Rect,
        projective: &Transform3D,
    ) -> Result<(), RenderError> {
        self.flush()?;
        self.preflight_projective(cmd, clip, projective)?;
        if projective.is_identity() {
            self.draw(cmd, clip);
            return self.flush();
        }
        let logical = projective.compose(&Transform3D::from_affine(cmd.transform()));
        match cmd {
            DrawCommand::Fill {
                area,
                color,
                radius,
                opa,
                ..
            } => {
                let quad = logical
                    .apply_rect(*area)
                    .ok_or(ProjectiveDrawError::InvalidProjection)?;
                self.dispatch_fill_quad(&quad, area, color, *radius, *opa, clip);
            }
            DrawCommand::Border {
                area,
                color,
                width,
                radius,
                opa,
                ..
            } => {
                let quad = logical
                    .apply_rect(*area)
                    .ok_or(ProjectiveDrawError::InvalidProjection)?;
                self.dispatch_border_quad(&quad, color, *width, *radius, *opa, clip);
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
                let quad = logical
                    .apply_rect(Rect {
                        x: pos.x,
                        y: pos.y,
                        w: size.x,
                        h: size.y,
                    })
                    .ok_or(ProjectiveDrawError::InvalidProjection)?;
                self.dispatch_blit_quad(&quad, texture, *size, clip, *radius, *opa, *composite);
            }
            DrawCommand::GlyphRun {
                pos,
                glyphs,
                font,
                color,
                opa,
                ..
            } => {
                let physical =
                    Transform3D::from_affine(self.viewport.as_transform()).compose(&logical);
                self.draw_glyph_run_projective_inner(label::ProjectiveRun {
                    pos,
                    glyphs,
                    font,
                    transform: &physical,
                    clip: self.physical_draw_clip(*clip),
                    color,
                    opacity: *opa,
                });
            }
            DrawCommand::PosedGlyphRun {
                pos,
                transform,
                glyphs,
                font,
                color,
                opa,
            } => {
                let physical_projective =
                    Transform3D::from_affine(self.viewport.as_transform()).compose(projective);
                self.draw_posed_glyph_run_projective_inner(label::ProjectivePosedRun {
                    pos,
                    glyphs: glyphs.glyphs(),
                    frames: glyphs.frames(),
                    font,
                    command_transform: transform,
                    projective_transform: &physical_projective,
                    clip: self.physical_draw_clip(*clip),
                    color,
                    opacity: *opa,
                });
            }
            DrawCommand::FillPath {
                path,
                paint,
                opa,
                fill_rule,
                ..
            } => {
                let physical =
                    Transform3D::from_affine(self.viewport.as_transform()).compose(&logical);
                self.fill_path_projective(
                    path,
                    &physical,
                    self.physical_draw_clip(*clip),
                    paint,
                    *opa,
                    *fill_rule,
                )?;
            }
            DrawCommand::StrokePath {
                path,
                transform,
                paint,
                width,
                opa,
                line_cap,
                line_join,
                miter_limit,
                dash,
            } => {
                let physical_projective =
                    Transform3D::from_affine(self.viewport.as_transform()).compose(projective);
                self.stroke_commands_projective(
                    path.commands(),
                    transform,
                    &physical_projective,
                    self.physical_draw_clip(*clip),
                    *width,
                    paint,
                    *opa,
                    *line_cap,
                    *line_join,
                    *miter_limit,
                    dash,
                )?;
            }
            DrawCommand::Line {
                p1,
                p2,
                transform,
                color,
                width,
                opa,
            } => {
                let commands = [PathCmd::MoveTo(*p1), PathCmd::LineTo(*p2)];
                let paint = Paint::Color((*color).into());
                let physical_projective =
                    Transform3D::from_affine(self.viewport.as_transform()).compose(projective);
                self.stroke_commands_projective(
                    &commands,
                    transform,
                    &physical_projective,
                    self.physical_draw_clip(*clip),
                    *width,
                    &paint,
                    *opa,
                    crate::render::raster::LineCap::Butt,
                    crate::render::raster::LineJoin::Miter,
                    Fixed::from_int(4),
                    &[],
                )?;
            }
            DrawCommand::Arc {
                center,
                transform,
                radius,
                start_angle,
                end_angle,
                color,
                width,
                opa,
            } => {
                let mut path = core::mem::take(&mut self.scratch.primitive_path);
                path.set_arc(*center, *radius, *start_angle, *end_angle);
                let paint = Paint::Color((*color).into());
                let physical_projective =
                    Transform3D::from_affine(self.viewport.as_transform()).compose(projective);
                let result = self.stroke_commands_projective(
                    path.commands(),
                    transform,
                    &physical_projective,
                    self.physical_draw_clip(*clip),
                    *width,
                    &paint,
                    *opa,
                    crate::render::raster::LineCap::Butt,
                    crate::render::raster::LineJoin::Miter,
                    Fixed::from_int(4),
                    &[],
                );
                self.scratch.primitive_path = path;
                result?;
            }
            DrawCommand::PushClip {
                path, fill_rule, ..
            } => {
                let physical =
                    Transform3D::from_affine(self.viewport.as_transform()).compose(&logical);
                self.push_clip_projective(path, &physical, *fill_rule)?;
            }
            DrawCommand::PopClip => self.pop_clip(),
            DrawCommand::ApplyBlur { .. } => return Err(ProjectiveDrawError::Unsupported.into()),
        }
        self.flush()
    }

    pub(crate) fn preflight_projective(
        &self,
        command: &DrawCommand,
        _clip: &Rect,
        projective: &Transform3D,
    ) -> Result<(), ProjectiveDrawError> {
        if projective.is_identity() {
            return Ok(());
        }
        let logical = projective.compose(&Transform3D::from_affine(command.transform()));
        if logical.inverse().is_none() {
            return Err(ProjectiveDrawError::InvalidProjection);
        }
        match command {
            DrawCommand::FillPath {
                path,
                paint: paint @ (Paint::LinearGradient(_) | Paint::RadialGradient(_)),
                ..
            } => {
                let Some(bbox) = path.bbox() else {
                    return Err(ProjectiveDrawError::InvalidProjection);
                };
                if crate::render::paint::ProjectiveGradientPaint::new(paint, logical, bbox)
                    .is_none()
                {
                    return Err(ProjectiveDrawError::InvalidProjection);
                }
            }
            DrawCommand::StrokePath {
                path,
                paint: paint @ (Paint::LinearGradient(_) | Paint::RadialGradient(_)),
                width,
                ..
            } => {
                let Some(bbox) = path.bbox() else {
                    return Err(ProjectiveDrawError::InvalidProjection);
                };
                let half = *width / 2;
                let bbox = Rect::new(
                    bbox.x - half,
                    bbox.y - half,
                    bbox.w + *width,
                    bbox.h + *width,
                );
                if crate::render::paint::ProjectiveGradientPaint::new(paint, logical, bbox)
                    .is_none()
                {
                    return Err(ProjectiveDrawError::InvalidProjection);
                }
            }
            _ => {}
        }
        let supported = match command {
            DrawCommand::Fill { .. }
            | DrawCommand::Border { .. }
            | DrawCommand::GlyphRun { .. }
            | DrawCommand::PosedGlyphRun { .. }
            | DrawCommand::PushClip { .. }
            | DrawCommand::PopClip => true,
            DrawCommand::Blit { .. } => true,
            DrawCommand::FillPath { .. } => true,
            DrawCommand::Line { .. } => true,
            DrawCommand::Arc { .. } => true,
            DrawCommand::StrokePath { .. } => true,
            DrawCommand::ApplyBlur { .. } => false,
        };
        if !supported {
            return Err(ProjectiveDrawError::Unsupported);
        }
        let valid_geometry = match command {
            DrawCommand::Fill { area, .. } | DrawCommand::Border { area, .. } => {
                logical.apply_rect(*area).is_some()
            }
            DrawCommand::Blit { pos, size, .. } => logical
                .apply_rect(Rect::new(pos.x, pos.y, size.x, size.y))
                .is_some(),
            DrawCommand::PosedGlyphRun {
                pos,
                glyphs,
                font,
                transform,
                ..
            } => glyphs
                .ink_bounds(font, *pos, *transform, self.viewport.scale())
                .is_none_or(|bounds| projective.apply_rect(bounds).is_some()),
            DrawCommand::GlyphRun {
                pos,
                transform,
                glyphs,
                font,
                ..
            } => {
                let physical =
                    Transform3D::from_affine(self.viewport.as_transform()).compose(&logical);
                let output_ppem = crate::render::font::output_ppem(
                    font.size.max(1),
                    physical.raster_scale_at(*pos),
                );
                font.glyph_run_ink_bounds(glyphs, *pos, *transform, output_ppem)
                    .is_none_or(|bounds| projective.apply_rect(bounds).is_some())
            }
            DrawCommand::FillPath { path, .. }
            | DrawCommand::StrokePath { path, .. }
            | DrawCommand::PushClip { path, .. } => path
                .bbox()
                .is_none_or(|bounds| logical.apply_rect(bounds).is_some()),
            DrawCommand::Line { p1, p2, .. } => {
                logical.apply_point(*p1).is_some() && logical.apply_point(*p2).is_some()
            }
            DrawCommand::Arc { center, radius, .. } => logical
                .apply_rect(Rect::new(
                    center.x - *radius,
                    center.y - *radius,
                    *radius * 2,
                    *radius * 2,
                ))
                .is_some(),
            DrawCommand::PopClip => true,
            DrawCommand::ApplyBlur { .. } => true,
        };
        if !valid_geometry {
            return Err(ProjectiveDrawError::InvalidProjection);
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        Canvas::flush(self)
    }

    fn supports_offscreen(&self) -> bool {
        true
    }

    fn offscreen_format(&self) -> Option<crate::render::texture::ColorFormat> {
        Some(self.target.format)
    }

    fn sample_target_region(
        &self,
        src: &Rect,
    ) -> Result<Option<crate::render::texture::Texture<'static>>, RenderError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        let (sx0, sy0, sx1, sy1) = self.viewport.rect_to_physical_pixel_bounds(*src);
        if sx0 >= i32::from(self.target.width)
            || sy0 >= i32::from(self.target.height)
            || sx1 <= 0
            || sy1 <= 0
        {
            return Ok(None);
        }
        let w = u16::try_from((sx1 - sx0).max(1)).map_err(|_| RenderError::InvalidGeometry)?;
        let h = u16::try_from((sy1 - sy0).max(1)).map_err(|_| RenderError::InvalidGeometry)?;
        let mut tex = crate::render::texture::Texture::owned(w, h, self.target.format)?;
        self.read_target_region(src, &mut tex)?;
        Ok(Some(tex))
    }

    fn modify_target_region(
        &mut self,
        src: &Rect,
        f: &mut dyn FnMut(&mut crate::render::texture::Texture) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        use crate::render::scratch::{PlaneLayout, PlaneRequirements};
        if let Some(error) = self.failure {
            return Err(error);
        }
        let (sx0, sy0, sx1, sy1) = self.viewport.rect_to_physical_pixel_bounds(*src);
        let target_w = self.target.width as i32;
        let target_h = self.target.height as i32;
        let cx0 = sx0.max(0);
        let cy0 = sy0.max(0);
        let cx1 = sx1.min(target_w);
        let cy1 = sy1.min(target_h);
        if cx1 <= cx0 || cy1 <= cy0 {
            return Ok(false);
        }
        let alpha_mode = self.target.alpha_mode;
        let mut target = self.target.pixels_mut()?;
        let format = target.format();
        let bpp = format.bytes_per_pixel();
        let target_stride = target.stride();
        let off_start = cy0 as usize * target_stride + cx0 as usize * bpp;
        let row_w = (cx1 - cx0) as u16;
        let row_h = (cy1 - cy0) as u16;
        let layout = PlaneLayout::new(row_w, row_h, format, target_stride, PlaneRequirements::CPU)
            .map_err(|_| RenderError::InvalidTexture)?;
        let buf = &mut target.bytes_mut()[off_start..off_start + layout.minimum_bytes()];
        let mut view = Texture::new(buf, row_w, row_h, format)?.with_stride(target_stride)?;
        view.alpha_mode = alpha_mode;
        f(&mut view)?;
        Ok(true)
    }

    fn supports_scroll_blit(&self) -> bool {
        true
    }

    fn scroll_target_region(
        &mut self,
        area: &Rect,
        dx: Fixed,
        dy: Fixed,
    ) -> Result<(), RenderError> {
        self.flush()?;
        // Truncate (not floor) so sub-pixel residue keeps the original sign.
        let Some(area) = self.viewport.physical_rect(*area) else {
            return Ok(());
        };
        let scale = self.viewport.scale();
        let dx_phys = (dx * scale).trunc_to_int();
        let dy_phys = (dy * scale).trunc_to_int();
        crate::surface::mirror::texture_scroll_in_place(&mut self.target, area, dx_phys, dy_phys)
    }

    fn read_target_region(
        &self,
        src: &Rect,
        dst: &mut crate::render::texture::Texture,
    ) -> Result<(), RenderError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        // Caller may pass a logical-sized dst; clipping to the
        // overlap avoids stretched top-left samples on HiDPI.
        let (sx0, sy0, sx1, sy1) = self.viewport.rect_to_physical_pixel_bounds(*src);
        let target = self.target.read_pixels()?;
        let mut dst = dst.pixels_mut()?;
        let target_w = target.width() as i32;
        let target_h = target.height() as i32;
        let copy_w = ((sx1 - sx0).min(dst.width() as i32)).max(0);
        let copy_h = ((sy1 - sy0).min(dst.height() as i32)).max(0);
        if copy_w == 0 || copy_h == 0 {
            return Ok(());
        }

        // Same-format reads can copy complete pixel rows.
        if target.direct_format() == Some(dst.format()) {
            let bpp = dst.format().bytes_per_pixel();
            let target_buf = target.bytes();
            let target_stride = target.stride();
            let dst_stride = dst.stride();
            let dst_buf = dst.bytes_mut();
            for dy in 0..copy_h as usize {
                let phys_y = sy0 + dy as i32;
                if phys_y < 0 || phys_y >= target_h {
                    continue;
                }
                let src_x_start = sx0.max(0);
                let src_x_end = (sx0 + copy_w).min(target_w);
                if src_x_end <= src_x_start {
                    continue;
                }
                let dst_x_start = (src_x_start - sx0) as usize;
                let row_bytes = (src_x_end - src_x_start) as usize * bpp;
                let src_row_off = phys_y as usize * target_stride + src_x_start as usize * bpp;
                let dst_row_off = dy * dst_stride + dst_x_start * bpp;
                dst_buf[dst_row_off..dst_row_off + row_bytes]
                    .copy_from_slice(&target_buf[src_row_off..src_row_off + row_bytes]);
            }
            return Ok(());
        }

        for dy in 0..copy_h {
            for dx in 0..copy_w {
                let phys_x = sx0 + dx;
                let phys_y = sy0 + dy;
                if phys_x < 0 || phys_y < 0 || phys_x >= target_w || phys_y >= target_h {
                    continue;
                }
                let px = target.get_pixel(phys_x, phys_y);
                dst.set_pixel(dx, dy, &px);
            }
        }
        Ok(())
    }
}

impl Renderer for SwRenderer<'_> {
    type TargetState = ();

    fn route(&self, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        SwRenderer::route(self, request)
    }

    fn prepare<'data>(
        &mut self,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Result<crate::render::PreparedDraw<'data>, RenderError>> {
        let result = crate::render::PreparedDraw::poll_cpu(request, route, self.output_scale(), cx);
        if let core::task::Poll::Ready(Err(error)) = result {
            self.failure.get_or_insert(error);
        }
        result
    }

    fn submit(&mut self, draw: &::mirui::render::PreparedDraw<'_>) -> Result<(), RenderError> {
        self.submit_prepared(draw)
    }

    fn poll_draw_request(
        &mut self,
        request: DrawRequest<'_, '_>,
        text_run: Option<crate::render::TextRunIdentity>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Result<(), RenderError>> {
        let route = self.route(&request)?;
        let draw = core::task::ready!(self.prepare(request, route, cx))?;
        let result = if let Some(identity) = text_run {
            self.submit_text_run(&draw, identity)
        } else if draw.projective().is_identity()
            && !matches!(draw.command(), DrawCommand::Blit { .. })
        {
            // Route checked this immutable request against the unchanged viewport.
            self.submit_routed_primitive(&draw.request())
        } else {
            self.submit(&draw)
        };
        core::task::Poll::Ready(result)
    }

    fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
        SwRenderer::flush(self)
    }

    fn output_scale(&self) -> Fixed {
        SwRenderer::output_scale(self)
    }

    fn plan_scope(&self, bounds: &Rect) -> Result<FallbackRegion, RenderError> {
        SwRenderer::plan_scope(self, bounds)
    }

    fn supports_offscreen(&self) -> bool {
        SwRenderer::supports_offscreen(self)
    }

    fn offscreen_format(&self) -> Option<crate::render::texture::ColorFormat> {
        SwRenderer::offscreen_format(self)
    }

    fn sample_target_region(&self, src: &Rect) -> Result<Option<Texture<'static>>, RenderError> {
        SwRenderer::sample_target_region(self, src)
    }

    fn modify_target_region(
        &mut self,
        src: &Rect,
        f: &mut dyn FnMut(&mut Texture) -> Result<(), RenderError>,
    ) -> Result<bool, RenderError> {
        SwRenderer::modify_target_region(self, src, f)
    }

    fn supports_scroll_blit(&self) -> bool {
        SwRenderer::supports_scroll_blit(self)
    }

    fn scroll_target_region(
        &mut self,
        area: &Rect,
        dx: Fixed,
        dy: Fixed,
    ) -> Result<(), RenderError> {
        SwRenderer::scroll_target_region(self, area, dx, dy)
    }

    fn read_target_region(&self, src: &Rect, dst: &mut Texture) -> Result<(), RenderError> {
        SwRenderer::read_target_region(self, src, dst)
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sw/mod.rs"]
mod tests;
