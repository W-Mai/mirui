use super::SwRenderer;
use crate::render::font::scalar::ScalarField;
use crate::render::font::sdf::SignedDistanceField;
use crate::render::font::{Font, Glyph, GlyphKind};
use crate::types::{Color, Fixed, Point, Rect, Transform, Transform3D, fixed::storage};

#[derive(Clone, Copy)]
struct GlyphRasterContext {
    requested_size: u16,
    viewport_scale: Fixed,
    mono_scale: i32,
    mono_height: i32,
    bounds: (i32, i32, i32, i32),
}

enum TransformedGlyph<'a> {
    Mono(&'a [u8]),
    Coverage(ScalarField<'a>),
    SignedDistance(SignedDistanceField<'a>),
}

pub(super) struct TransformedRun<'a> {
    pub pos: &'a Point,
    pub glyphs: &'a [textflow::shaping::PositionedGlyph],
    pub font: &'a Font,
    pub transform: &'a Transform,
    pub clip: Rect,
    pub color: &'a Color,
    pub opacity: u8,
}

pub(super) struct PosedRun<'a> {
    pub pos: &'a Point,
    pub glyphs: &'a [textflow::shaping::PositionedGlyph],
    pub frames: &'a [textflow::placement::GlyphFrame],
    pub font: &'a Font,
    pub transform: &'a Transform,
    pub clip: Rect,
    pub color: &'a Color,
    pub opacity: u8,
}

pub(super) struct ProjectiveRun<'a> {
    pub pos: &'a Point,
    pub glyphs: &'a [textflow::shaping::PositionedGlyph],
    pub font: &'a Font,
    pub transform: &'a Transform3D,
    pub clip: Rect,
    pub color: &'a Color,
    pub opacity: u8,
}

pub(super) struct ProjectivePosedRun<'a> {
    pub pos: &'a Point,
    pub glyphs: &'a [textflow::shaping::PositionedGlyph],
    pub frames: &'a [textflow::placement::GlyphFrame],
    pub font: &'a Font,
    pub command_transform: &'a Transform,
    pub projective_transform: &'a Transform3D,
    pub clip: Rect,
    pub color: &'a Color,
    pub opacity: u8,
}

impl TransformedGlyph<'_> {
    fn dimensions(&self) -> (u32, u32) {
        match self {
            Self::Mono(bitmap) => (8, bitmap.len() as u32),
            Self::Coverage(field) => (field.width(), field.height()),
            Self::SignedDistance(field) => (field.width(), field.height()),
        }
    }
}

impl SwRenderer<'_> {
    pub(super) fn draw_glyph_run_inner(
        &mut self,
        pos: &Point,
        glyphs: &[textflow::shaping::PositionedGlyph],
        font: &Font,
        clip: &Rect,
        color: &Color,
        opa: u8,
    ) {
        let Some(first) = glyphs.first() else {
            return;
        };
        let phys_pos = self.viewport.point_to_physical(*pos);
        let viewport_scale = self.viewport.scale();
        let requested_size = font.size.max(1);
        let output_ppem = crate::render::font::output_ppem(requested_size, viewport_scale);
        let metrics = font.metrics(requested_size);
        let (base_x, base_y) = phys_pos.floor();
        let base_baseline = base_y + (metrics.ascender * viewport_scale).to_int();
        let context = GlyphRasterContext {
            requested_size,
            viewport_scale,
            mono_scale: viewport_scale.to_int().max(1),
            mono_height: metrics.line_height.to_int().max(1),
            bounds: self.viewport.rect_to_physical(*clip).pixel_bounds(),
        };
        for positioned in glyphs {
            let Some(glyph) =
                font.glyph_by_id_for_output(positioned.glyph_id(), requested_size, output_ppem)
            else {
                continue;
            };
            let Some(dx) = positioned
                .origin
                .x
                .checked_sub(first.origin.x)
                .and_then(|value| value.checked_add(positioned.offset.x))
            else {
                continue;
            };
            let Some(dy) = positioned
                .origin
                .y
                .checked_sub(first.origin.y)
                .and_then(|value| value.checked_add(positioned.offset.y))
            else {
                continue;
            };
            let dx = crate::types::fixed::from_textflow(dx);
            let dy = crate::types::fixed::from_textflow(dy);
            let x = base_x + (dx * viewport_scale).to_int();
            let baseline = base_baseline + (dy * viewport_scale).to_int();
            let mono_y = baseline - (metrics.ascender * viewport_scale).to_int();
            self.draw_glyph_at(&glyph, x, mono_y, baseline, context, color, opa);
        }
    }

    pub(super) fn draw_glyph_run_transformed_inner(&mut self, run: TransformedRun<'_>) {
        let (Some(first), Some(inverse)) = (run.glyphs.first(), run.transform.inverse()) else {
            return;
        };
        let requested_size = run.font.size.max(1);
        let output_ppem =
            crate::render::font::output_ppem(requested_size, run.transform.raster_scale());
        let metrics = run.font.metrics(requested_size);
        for positioned in run.glyphs {
            let Some(glyph) =
                run.font
                    .glyph_by_id_for_output(positioned.glyph_id(), requested_size, output_ppem)
            else {
                continue;
            };
            let Some(dx) = positioned
                .origin
                .x
                .checked_sub(first.origin.x)
                .and_then(|value| value.checked_add(positioned.offset.x))
            else {
                continue;
            };
            let Some(dy) = positioned
                .origin
                .y
                .checked_sub(first.origin.y)
                .and_then(|value| value.checked_add(positioned.offset.y))
            else {
                continue;
            };
            let x = run.pos.x + crate::types::fixed::from_textflow(dx);
            let baseline = run.pos.y + metrics.ascender + crate::types::fixed::from_textflow(dy);
            match glyph.kind {
                GlyphKind::Mono(bitmap) => {
                    let rect = Rect {
                        x,
                        y: baseline - metrics.ascender,
                        w: Fixed::from_int(8),
                        h: metrics.line_height,
                    };
                    self.blit_transformed_glyph(
                        TransformedGlyph::Mono(bitmap),
                        rect,
                        run.transform,
                        &inverse,
                        run.clip,
                        run.color,
                        run.opacity,
                    );
                }
                GlyphKind::Raster {
                    samples,
                    stride,
                    region,
                    representation,
                    bearing_x,
                    bearing_y,
                } => {
                    if region.width() == 0 || region.height() == 0 {
                        continue;
                    }
                    let scale = Fixed::from_int(i32::from(requested_size))
                        / Fixed::from_int(i32::from(representation.design_ppem().max(1)));
                    let rect = Rect {
                        x: x + bearing_x,
                        y: baseline - bearing_y,
                        w: Fixed::from_int(region.width() as i32) * scale,
                        h: Fixed::from_int(region.height() as i32) * scale,
                    };
                    let field = match representation.kind() {
                        mirx::font::FontRepresentationKind::Coverage { bits } => {
                            ScalarField::new(samples, stride, region, bits)
                                .map(TransformedGlyph::Coverage)
                        }
                        mirx::font::FontRepresentationKind::SignedDistance { bits, spread } => {
                            SignedDistanceField::new(samples, stride, region, bits, spread)
                                .map(TransformedGlyph::SignedDistance)
                        }
                        _ => None,
                    };
                    if let Some(field) = field {
                        self.blit_transformed_glyph(
                            field,
                            rect,
                            run.transform,
                            &inverse,
                            run.clip,
                            run.color,
                            run.opacity,
                        );
                    }
                }
            }
        }
    }

    pub(super) fn draw_posed_glyph_run_inner(&mut self, run: PosedRun<'_>) {
        let requested_size = run.font.size.max(1);
        for (positioned, frame) in run.glyphs.iter().zip(run.frames) {
            let output_ppem =
                crate::render::font::output_ppem(requested_size, run.transform.raster_scale());
            let Some(raster) =
                run.font
                    .raster_for_output(positioned.glyph_id(), requested_size, output_ppem)
            else {
                continue;
            };
            let Some(quad) = raster.posed_quad(*run.pos, *frame, requested_size, *run.transform)
            else {
                continue;
            };
            let Some(inverse) = quad.transform.inverse() else {
                continue;
            };
            let Some(region) = raster.region else {
                continue;
            };
            let field = match raster.representation.kind() {
                mirx::font::FontRepresentationKind::Coverage { bits } => ScalarField::new(
                    raster.surface.samples(),
                    raster.surface.stride(),
                    region,
                    bits,
                )
                .map(TransformedGlyph::Coverage),
                mirx::font::FontRepresentationKind::SignedDistance { bits, spread } => {
                    SignedDistanceField::new(
                        raster.surface.samples(),
                        raster.surface.stride(),
                        region,
                        bits,
                        spread,
                    )
                    .map(TransformedGlyph::SignedDistance)
                }
                _ => None,
            };
            if let Some(field) = field {
                self.blit_transformed_glyph(
                    field,
                    quad.rect,
                    &quad.transform,
                    &inverse,
                    run.clip,
                    run.color,
                    run.opacity,
                );
            }
        }
    }

    pub(super) fn draw_glyph_run_projective_inner(&mut self, run: ProjectiveRun<'_>) {
        let (Some(first), Some(inverse)) = (run.glyphs.first(), run.transform.inverse()) else {
            return;
        };
        let requested_size = run.font.size.max(1);
        let output_ppem = crate::render::font::output_ppem(
            requested_size,
            projective_scale_at(run.transform, *run.pos),
        );
        let metrics = run.font.metrics(requested_size);
        for positioned in run.glyphs {
            let Some(glyph) =
                run.font
                    .glyph_by_id_for_output(positioned.glyph_id(), requested_size, output_ppem)
            else {
                continue;
            };
            let Some(dx) = positioned
                .origin
                .x
                .checked_sub(first.origin.x)
                .and_then(|value| value.checked_add(positioned.offset.x))
            else {
                continue;
            };
            let Some(dy) = positioned
                .origin
                .y
                .checked_sub(first.origin.y)
                .and_then(|value| value.checked_add(positioned.offset.y))
            else {
                continue;
            };
            let x = run.pos.x + crate::types::fixed::from_textflow(dx);
            let baseline = run.pos.y + metrics.ascender + crate::types::fixed::from_textflow(dy);
            match glyph.kind {
                GlyphKind::Mono(bitmap) => {
                    let rect = Rect {
                        x,
                        y: baseline - metrics.ascender,
                        w: Fixed::from_int(8),
                        h: metrics.line_height,
                    };
                    self.blit_projective_glyph(
                        TransformedGlyph::Mono(bitmap),
                        rect,
                        run.transform,
                        &inverse,
                        run.clip,
                        run.color,
                        run.opacity,
                    );
                }
                GlyphKind::Raster {
                    samples,
                    stride,
                    region,
                    representation,
                    bearing_x,
                    bearing_y,
                } => {
                    if region.width() == 0 || region.height() == 0 {
                        continue;
                    }
                    let scale = Fixed::from_int(i32::from(requested_size))
                        / Fixed::from_int(i32::from(representation.design_ppem().max(1)));
                    let rect = Rect {
                        x: x + bearing_x,
                        y: baseline - bearing_y,
                        w: Fixed::from_int(region.width() as i32) * scale,
                        h: Fixed::from_int(region.height() as i32) * scale,
                    };
                    let field = match representation.kind() {
                        mirx::font::FontRepresentationKind::Coverage { bits } => {
                            ScalarField::new(samples, stride, region, bits)
                                .map(TransformedGlyph::Coverage)
                        }
                        mirx::font::FontRepresentationKind::SignedDistance { bits, spread } => {
                            SignedDistanceField::new(samples, stride, region, bits, spread)
                                .map(TransformedGlyph::SignedDistance)
                        }
                        _ => None,
                    };
                    if let Some(field) = field {
                        self.blit_projective_glyph(
                            field,
                            rect,
                            run.transform,
                            &inverse,
                            run.clip,
                            run.color,
                            run.opacity,
                        );
                    }
                }
            }
        }
    }

    pub(super) fn draw_posed_glyph_run_projective_inner(&mut self, run: ProjectivePosedRun<'_>) {
        let requested_size = run.font.size.max(1);
        for (positioned, frame) in run.glyphs.iter().zip(run.frames) {
            let output_ppem = crate::render::font::output_ppem(
                requested_size,
                projective_scale_at(run.projective_transform, *run.pos),
            );
            let Some(raster) =
                run.font
                    .raster_for_output(positioned.glyph_id(), requested_size, output_ppem)
            else {
                continue;
            };
            let Some(quad) =
                raster.posed_quad(*run.pos, *frame, requested_size, *run.command_transform)
            else {
                continue;
            };
            let combined = run
                .projective_transform
                .compose(&Transform3D::from_affine(quad.transform));
            let Some(inverse) = combined.inverse() else {
                continue;
            };
            let Some(region) = raster.region else {
                continue;
            };
            let field = match raster.representation.kind() {
                mirx::font::FontRepresentationKind::Coverage { bits } => ScalarField::new(
                    raster.surface.samples(),
                    raster.surface.stride(),
                    region,
                    bits,
                )
                .map(TransformedGlyph::Coverage),
                mirx::font::FontRepresentationKind::SignedDistance { bits, spread } => {
                    SignedDistanceField::new(
                        raster.surface.samples(),
                        raster.surface.stride(),
                        region,
                        bits,
                        spread,
                    )
                    .map(TransformedGlyph::SignedDistance)
                }
                _ => None,
            };
            if let Some(field) = field {
                self.blit_projective_glyph(
                    field,
                    quad.rect,
                    &combined,
                    &inverse,
                    run.clip,
                    run.color,
                    run.opacity,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blit_projective_glyph(
        &mut self,
        glyph: TransformedGlyph<'_>,
        logical_rect: Rect,
        physical_transform: &Transform3D,
        inverse: &Transform3D,
        physical_clip: Rect,
        color: &Color,
        opa: u8,
    ) {
        if logical_rect.w <= Fixed::ZERO || logical_rect.h <= Fixed::ZERO {
            return;
        }
        let Some(draw_area) = physical_transform
            .apply_rect(logical_rect)
            .map(|quad| Rect::bounding_quad(&quad))
            .and_then(|area| area.intersect(&physical_clip))
            .and_then(|area| {
                area.intersect(&Rect::new(0, 0, self.target.width, self.target.height))
            })
        else {
            return;
        };
        let (source_width, source_height) = glyph.dimensions();
        let source_scale_x = Fixed::from_int(source_width as i32) / logical_rect.w;
        let source_scale_y = Fixed::from_int(source_height as i32) / logical_rect.h;
        let (x0, y0, x1, y1) = draw_area.pixel_bounds();
        let target_width = self.target.width as usize;
        let clip_mask = self.clip_stack.last().map(|mask| mask.alpha.as_slice());
        for py in y0..y1 {
            let mask_row = py as usize * target_width;
            for px in x0..x1 {
                let screen = Point {
                    x: Fixed::from_int(px) + Fixed::HALF,
                    y: Fixed::from_int(py) + Fixed::HALF,
                };
                let Some(logical) = inverse.apply_point(screen) else {
                    continue;
                };
                let u = logical.x - logical_rect.x;
                let v = logical.y - logical_rect.y;
                if u < Fixed::ZERO || v < Fixed::ZERO || u >= logical_rect.w || v >= logical_rect.h
                {
                    continue;
                }
                let sx = u * source_scale_x - Fixed::HALF;
                let sy = v * source_scale_y - Fixed::HALF;
                let coverage = match &glyph {
                    TransformedGlyph::Mono(bitmap) => sample_mono_bilinear(bitmap, sx, sy),
                    TransformedGlyph::Coverage(field) => field.sample_bilinear(sx, sy),
                    TransformedGlyph::SignedDistance(field) => {
                        let (distance, gradient_x, gradient_y) = field.sample_with_gradient(sx, sy);
                        let Some(logical_x) = inverse.apply_point(Point {
                            x: screen.x + Fixed::ONE,
                            y: screen.y,
                        }) else {
                            continue;
                        };
                        let Some(logical_y) = inverse.apply_point(Point {
                            x: screen.x,
                            y: screen.y + Fixed::ONE,
                        }) else {
                            continue;
                        };
                        let source_dx_x = (logical_x.x - logical.x) * source_scale_x;
                        let source_dx_y = (logical_x.y - logical.y) * source_scale_y;
                        let source_dy_x = (logical_y.x - logical.x) * source_scale_x;
                        let source_dy_y = (logical_y.y - logical.y) * source_scale_y;
                        let screen_x = gradient_x * source_dx_x + gradient_y * source_dx_y;
                        let screen_y = gradient_x * source_dy_x + gradient_y * source_dy_y;
                        let edge_half = ((screen_x * screen_x + screen_y * screen_y).sqrt() / 2)
                            .max(Fixed::from_ratio(1, 256));
                        ((distance + edge_half) / (edge_half * 2))
                            .max(Fixed::ZERO)
                            .min(Fixed::ONE)
                    }
                };
                if coverage <= Fixed::ZERO {
                    continue;
                }
                let mut alpha = (coverage * Fixed::from_int(i32::from(opa)))
                    .to_int()
                    .clamp(0, 255) as u8;
                if let Some(mask) = clip_mask {
                    alpha = ((u16::from(alpha) * u16::from(mask[mask_row + px as usize]) + 127)
                        / 255) as u8;
                }
                if alpha != 0 {
                    self.target.blend_pixel_int(px, py, color, alpha);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blit_transformed_glyph(
        &mut self,
        glyph: TransformedGlyph<'_>,
        logical_rect: Rect,
        physical_transform: &Transform,
        inverse: &Transform,
        physical_clip: Rect,
        color: &Color,
        opa: u8,
    ) {
        if logical_rect.w <= Fixed::ZERO || logical_rect.h <= Fixed::ZERO {
            return;
        }
        let Some(draw_area) = physical_transform
            .apply_rect_bbox(logical_rect)
            .intersect(&physical_clip)
            .and_then(|area| {
                area.intersect(&Rect::new(0, 0, self.target.width, self.target.height))
            })
        else {
            return;
        };
        let (source_width, source_height) = glyph.dimensions();
        let source_scale_x = Fixed::from_int(source_width as i32) / logical_rect.w;
        let source_scale_y = Fixed::from_int(source_height as i32) / logical_rect.h;
        let source_dx_x = inverse.m00 * source_scale_x;
        let source_dx_y = inverse.m10 * source_scale_y;
        let source_dy_x = inverse.m01 * source_scale_x;
        let source_dy_y = inverse.m11 * source_scale_y;
        let (x0, y0, x1, y1) = draw_area.pixel_bounds();
        let target_width = self.target.width as usize;
        let clip_mask = self.clip_stack.last().map(|mask| mask.alpha.as_slice());
        for py in y0..y1 {
            let mask_row = py as usize * target_width;
            for px in x0..x1 {
                let logical = inverse.apply_point(Point {
                    x: Fixed::from_int(px) + Fixed::HALF,
                    y: Fixed::from_int(py) + Fixed::HALF,
                });
                let u = logical.x - logical_rect.x;
                let v = logical.y - logical_rect.y;
                if u < Fixed::ZERO || v < Fixed::ZERO || u >= logical_rect.w || v >= logical_rect.h
                {
                    continue;
                }
                let sx = u * source_scale_x - Fixed::HALF;
                let sy = v * source_scale_y - Fixed::HALF;
                let coverage = match &glyph {
                    TransformedGlyph::Mono(bitmap) => sample_mono_bilinear(bitmap, sx, sy),
                    TransformedGlyph::Coverage(field) => field.sample_bilinear(sx, sy),
                    TransformedGlyph::SignedDistance(field) => {
                        let (distance, gradient_x, gradient_y) = field.sample_with_gradient(sx, sy);
                        let screen_x = gradient_x * source_dx_x + gradient_y * source_dx_y;
                        let screen_y = gradient_x * source_dy_x + gradient_y * source_dy_y;
                        let edge_half = ((screen_x * screen_x + screen_y * screen_y).sqrt() / 2)
                            .max(Fixed::from_ratio(1, 256));
                        ((distance + edge_half) / (edge_half * 2))
                            .max(Fixed::ZERO)
                            .min(Fixed::ONE)
                    }
                };
                if coverage <= Fixed::ZERO {
                    continue;
                }
                let mut alpha = (coverage * Fixed::from_int(i32::from(opa)))
                    .to_int()
                    .clamp(0, 255) as u8;
                if let Some(mask) = clip_mask {
                    alpha = ((u16::from(alpha) * u16::from(mask[mask_row + px as usize]) + 127)
                        / 255) as u8;
                }
                if alpha != 0 {
                    self.target.blend_pixel_int(px, py, color, alpha);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_glyph_at(
        &mut self,
        glyph: &Glyph<'_>,
        x: i32,
        mono_y: i32,
        baseline: i32,
        context: GlyphRasterContext,
        color: &Color,
        opa: u8,
    ) {
        match &glyph.kind {
            GlyphKind::Mono(bitmap) => self.blit_mono_glyph(
                bitmap,
                x,
                mono_y,
                context.mono_scale,
                context.mono_height,
                context.bounds,
                color,
                opa,
            ),
            GlyphKind::Raster {
                samples,
                stride,
                region,
                representation,
                bearing_x,
                bearing_y,
            } => {
                if region.width() == 0 || region.height() == 0 {
                    return;
                }
                let design = representation.design_ppem().max(1);
                let glyph_scale = Fixed::from_int(i32::from(context.requested_size))
                    / Fixed::from_int(i32::from(design))
                    * context.viewport_scale;
                let raster_x = x + (*bearing_x * context.viewport_scale).to_int();
                let raster_y = baseline - (*bearing_y * context.viewport_scale).to_int();
                let width = scaled_extent(region.width(), glyph_scale);
                let height = scaled_extent(region.height(), glyph_scale);
                match representation.kind() {
                    mirx::font::FontRepresentationKind::Coverage { bits } => self
                        .blit_coverage_region(
                            samples,
                            *stride,
                            *region,
                            bits,
                            raster_x,
                            raster_y,
                            width,
                            height,
                            context.bounds,
                            color,
                            opa,
                        ),
                    mirx::font::FontRepresentationKind::SignedDistance { bits, spread } => self
                        .blit_sdf_region(
                            samples,
                            *stride,
                            *region,
                            bits,
                            spread,
                            raster_x,
                            raster_y,
                            width,
                            height,
                            context.bounds,
                            color,
                            opa,
                        ),
                    mirx::font::FontRepresentationKind::Application(_) => {}
                    _ => {}
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn blit_mono_glyph(
        &mut self,
        bitmap: &[u8],
        cx: i32,
        cy: i32,
        scale: i32,
        char_h: i32,
        phys_bounds: (i32, i32, i32, i32),
        color: &Color,
        opa: u8,
    ) {
        let (clip_x, clip_y, clip_x2, clip_y2) = phys_bounds;
        let target_w = self.target.width as usize;
        let clip_mask = self.clip_stack.last().map(|m| m.alpha.as_slice());
        for row in 0..char_h.min(bitmap.len() as i32) {
            let byte = bitmap[row as usize];
            for col in 0..8 {
                if byte & (0x80 >> col) == 0 {
                    continue;
                }
                for sy in 0..scale {
                    for sx in 0..scale {
                        let px = cx + col * scale + sx;
                        let py = cy + row * scale + sy;
                        if px >= clip_x && px < clip_x2 && py >= clip_y && py < clip_y2 {
                            let alpha = match clip_mask {
                                Some(m) => {
                                    let ca = m[py as usize * target_w + px as usize];
                                    if ca == 0 {
                                        continue;
                                    }
                                    ((opa as u16 * ca as u16 + 127) / 255) as u8
                                }
                                None => opa,
                            };
                            self.target.blend_pixel(
                                Fixed::from_int(px),
                                Fixed::from_int(py),
                                color,
                                alpha,
                            );
                        }
                    }
                }
            }
        }
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    fn blit_coverage_glyph(
        &mut self,
        coverage: &[u8],
        bpp: u8,
        w: u8,
        h: u8,
        x0: i32,
        y0: i32,
        phys_bounds: (i32, i32, i32, i32),
        color: &Color,
        base_opa: u8,
    ) {
        let layout = match bpp {
            1 => mirx::image::SampleLayout::A1,
            2 => mirx::image::SampleLayout::A2,
            4 => mirx::image::SampleLayout::A4,
            8 => mirx::image::SampleLayout::A8,
            _ => return,
        };
        let surface = mirx::image::SurfaceDescriptor::new(
            w.into(),
            h.into(),
            layout,
            mirx::image::ColorDescription::NONE,
        )
        .unwrap();
        let region = surface.region(0, 0, w.into(), h.into()).unwrap();
        let stride = (u32::from(w) * u32::from(bpp)).div_ceil(8);
        self.blit_coverage_region(
            coverage,
            stride,
            region,
            bpp,
            x0,
            y0,
            u16::from(w),
            u16::from(h),
            phys_bounds,
            color,
            base_opa,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn blit_coverage_region(
        &mut self,
        coverage: &[u8],
        stride: u32,
        region: mirx::image::Region,
        bpp: u8,
        x0: i32,
        y0: i32,
        target_width: u16,
        target_height: u16,
        phys_bounds: (i32, i32, i32, i32),
        color: &Color,
        base_opa: u8,
    ) {
        let (clip_x, clip_y, clip_x2, clip_y2) = phys_bounds;
        let source_width = region.width();
        let source_height = region.height();
        if source_width == 0 || source_height == 0 {
            return;
        }
        let target_width = u32::from(target_width.max(1));
        let target_height = u32::from(target_height.max(1));
        let Some(field) = ScalarField::new(coverage, stride, region, bpp) else {
            return;
        };
        let scale_x = Fixed::from_int(source_width as i32) / Fixed::from_int(target_width as i32);
        let scale_y = Fixed::from_int(source_height as i32) / Fixed::from_int(target_height as i32);
        let half_texel = Fixed::HALF;
        let target_w = self.target.width as usize;
        let clip_mask = self.clip_stack.last().map(|m| m.alpha.as_slice());
        for row in 0..target_height {
            let source_y = (Fixed::from_int(row as i32) + half_texel) * scale_y - half_texel;
            for col in 0..target_width {
                let source_x = (Fixed::from_int(col as i32) + half_texel) * scale_x - half_texel;
                let coverage = field.sample_bilinear(source_x, source_y);
                if coverage <= Fixed::ZERO {
                    continue;
                }
                let mut alpha = (coverage * Fixed::from_int(i32::from(base_opa)))
                    .to_int()
                    .clamp(0, 255) as u8;
                if alpha == 0 {
                    continue;
                }
                let px = x0 + col as i32;
                let py = y0 + row as i32;
                if px >= clip_x && px < clip_x2 && py >= clip_y && py < clip_y2 {
                    if let Some(mask) = clip_mask {
                        let ca = mask[py as usize * target_w + px as usize];
                        if ca == 0 {
                            continue;
                        }
                        if ca < 255 {
                            alpha = ((alpha as u16 * ca as u16 + 127) / 255) as u8;
                            if alpha == 0 {
                                continue;
                            }
                        }
                    }
                    self.target.blend_pixel_int(px, py, color, alpha);
                }
            }
        }
    }
}

fn sample_mono_bilinear(bitmap: &[u8], x: Fixed, y: Fixed) -> Fixed {
    let x0 = x.floor().to_int();
    let y0 = y.floor().to_int();
    let tx = x - Fixed::from_int(x0);
    let ty = y - Fixed::from_int(y0);
    let top_left = mono_sample(bitmap, x0, y0);
    let top = top_left + (mono_sample(bitmap, x0 + 1, y0) - top_left) * tx;
    let bottom_left = mono_sample(bitmap, x0, y0 + 1);
    let bottom = bottom_left + (mono_sample(bitmap, x0 + 1, y0 + 1) - bottom_left) * tx;
    top + (bottom - top) * ty
}

fn mono_sample(bitmap: &[u8], x: i32, y: i32) -> Fixed {
    if !(0..8).contains(&x) || y < 0 || y as usize >= bitmap.len() {
        return Fixed::ZERO;
    }
    if bitmap[y as usize] & (0x80 >> x) == 0 {
        Fixed::ZERO
    } else {
        Fixed::ONE
    }
}

fn scaled_extent(extent: u32, scale: Fixed) -> u16 {
    let raw_scale = u64::try_from(storage::to_i32(scale)).unwrap_or(0);
    let pixels = (u64::from(extent) * raw_scale).div_ceil(256);
    pixels.clamp(1, u64::from(u16::MAX)) as u16
}

pub(super) fn projective_scale_at(transform: &Transform3D, point: Point) -> Fixed {
    let Some(origin) = transform.apply_point(point) else {
        return Fixed::ONE;
    };
    let x = transform
        .apply_point(Point {
            x: point.x + Fixed::ONE,
            y: point.y,
        })
        .map(|p| {
            let dx = p.x - origin.x;
            let dy = p.y - origin.y;
            (dx * dx + dy * dy).sqrt()
        })
        .unwrap_or(Fixed::ONE);
    let y = transform
        .apply_point(Point {
            x: point.x,
            y: point.y + Fixed::ONE,
        })
        .map(|p| {
            let dx = p.x - origin.x;
            let dy = p.y - origin.y;
            (dx * dx + dy * dy).sqrt()
        })
        .unwrap_or(Fixed::ONE);
    x.max(y).max(Fixed::from_ratio(1, 256))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::backends::sw::SwRenderer;
    use crate::render::font::{
        FontBackend, FontFaceId, FontMetrics, FontProvider, FontSurfaceId, GlyphId, GlyphSurface,
        RasterGlyph,
    };
    use crate::render::texture::{ColorFormat, Texture};
    use crate::types::Viewport;
    use alloc::rc::Rc;
    use alloc::vec;
    use core::cell::Cell;

    fn pixel_alpha(buf: &[u8], stride: usize, x: usize, y: usize) -> u8 {
        buf[(y * stride + x) * 4 + 3]
    }

    #[test]
    fn transformed_mono_sampling_filters_edges_and_transparent_border() {
        let bitmap = [0b1000_0000];

        assert_eq!(
            sample_mono_bilinear(&bitmap, Fixed::ZERO, Fixed::ZERO),
            Fixed::ONE
        );
        assert_eq!(
            sample_mono_bilinear(&bitmap, Fixed::HALF, Fixed::ZERO),
            Fixed::HALF
        );
        assert_eq!(
            sample_mono_bilinear(&bitmap, -Fixed::HALF, Fixed::ZERO),
            Fixed::HALF
        );
        assert_eq!(
            sample_mono_bilinear(&bitmap, Fixed::HALF, Fixed::HALF),
            Fixed::from_ratio(1, 4)
        );
    }

    #[test]
    fn glyph_run_uses_layout_origins_instead_of_remeasuring_text() {
        use textflow::bidi::BaseDirection;
        use textflow::shaping::Typeface;

        use crate::render::font::FontTypeface;
        use crate::text::layout::{TextLayoutCache, TextLayoutRequest};

        let font = Font::bitmap_8x8();
        let face = FontTypeface::new(&font, font.size);
        let faces: [&dyn Typeface; 1] = [&face];
        let mut cache = TextLayoutCache::default();
        cache.begin_frame();
        let handle = cache
            .layout(
                TextLayoutRequest {
                    text: "AB",
                    max_width: i32::MAX,
                    width: None,
                    max_lines: usize::MAX,
                    line_height: 8 * 256,
                    baseline: 7 * 256,
                    direction: BaseDirection::LeftToRight,
                    wrap: textflow::layout::WrapMode::NoWrap,
                    alignment: textflow::layout::Alignment::Start,
                    overflow: textflow::layout::Overflow::Clip,
                    spacing: textflow::layout::TextSpacing::default(),
                    features: &[],
                    line_widths: None,
                },
                &faces,
            )
            .unwrap();
        let mut glyphs = cache.get(handle).unwrap().glyphs().to_vec();
        glyphs[1].origin.x += 5 * 256;
        let clip = Rect::new(0, 0, 24, 8);
        let color = Color::rgba(255, 255, 255, 255);

        let mut actual = vec![0u8; 24 * 8 * 4];
        let texture = Texture::new(&mut actual, 24, 8, ColorFormat::RGBA8888);
        SwRenderer::new(texture).draw_glyph_run_inner(
            &Point::ZERO,
            &glyphs,
            &font,
            &clip,
            &color,
            255,
        );

        let mut expected = vec![0u8; 24 * 8 * 4];
        let texture = Texture::new(&mut expected, 24, 8, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(texture);
        let a = [textflow::shaping::PositionedGlyph::new(
            GlyphId::new(65),
            textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
        )];
        renderer.draw_glyph_run_inner(&Point::ZERO, &a, &font, &clip, &color, 255);
        let b = [textflow::shaping::PositionedGlyph::new(
            GlyphId::new(66),
            textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
        )];
        renderer.draw_glyph_run_inner(
            &Point {
                x: Fixed::from_int(13),
                y: Fixed::ZERO,
            },
            &b,
            &font,
            &clip,
            &color,
            255,
        );

        assert_eq!(actual, expected);
    }

    #[test]
    fn affine_glyph_run_renders_inside_its_transformed_quad() {
        let font = Font::bitmap_8x8();
        let glyphs = [textflow::shaping::PositionedGlyph::new(
            GlyphId::new(u16::from(b'A')),
            textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
        )];
        let transform = Transform::translate(Fixed::from_int(12), Fixed::from_int(1))
            .compose(&Transform::rotate_deg(Fixed::from_int(90)));
        let mut buf = vec![0u8; 16 * 16 * 4];
        let texture = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(texture);

        renderer.draw_glyph_run_transformed_inner(TransformedRun {
            pos: &Point::ZERO,
            glyphs: &glyphs,
            font: &font,
            transform: &transform,
            clip: Rect::new(0, 0, 16, 16),
            color: &Color::rgba(255, 255, 255, 255),
            opacity: 255,
        });

        let painted: alloc::vec::Vec<_> = buf
            .chunks_exact(4)
            .enumerate()
            .filter_map(|(index, pixel)| (pixel[0] != 0).then_some((index % 16, index / 16)))
            .collect();
        assert!(!painted.is_empty());
        assert!(
            painted
                .iter()
                .all(|&(x, y)| (4..12).contains(&x) && (1..9).contains(&y))
        );
    }

    #[test]
    fn posed_glyph_run_uses_each_glyph_tangent() {
        let font = Font::bitmap_8x8();
        let glyphs = [textflow::shaping::PositionedGlyph::new(
            GlyphId::new(u16::from(b'A')),
            textflow::shaping::FlowPoint { x: 0, y: 7 << 8 },
        )];
        let frames = [textflow::placement::GlyphFrame {
            local_origin: textflow::shaping::FlowPoint {
                x: 12 << 8,
                y: 1 << 8,
            },
            unit_tangent: textflow::shaping::FlowPoint { x: 0, y: 1 << 8 },
        }];
        let mut buf = vec![0u8; 16 * 16 * 4];
        let texture = Texture::new(&mut buf, 16, 16, ColorFormat::RGBA8888);
        let mut renderer = SwRenderer::new(texture);

        renderer.draw_posed_glyph_run_inner(PosedRun {
            pos: &Point::ZERO,
            glyphs: &glyphs,
            frames: &frames,
            font: &font,
            transform: &Transform::IDENTITY,
            clip: Rect::new(0, 0, 16, 16),
            color: &Color::rgba(255, 255, 255, 255),
            opacity: 255,
        });

        let painted: alloc::vec::Vec<_> = buf
            .chunks_exact(4)
            .enumerate()
            .filter_map(|(index, pixel)| (pixel[0] != 0).then_some((index % 16, index / 16)))
            .collect();
        assert!(!painted.is_empty());
        assert!(
            painted
                .iter()
                .all(|&(x, y)| (11..16).contains(&x) && (1..9).contains(&y))
        );
    }

    #[test]
    fn coverage_full_value_is_opaque_and_zero_is_blank() {
        let coverage = [0xF0_u8];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_coverage_glyph(&coverage, 4, 2, 1, 0, 0, (0, 0, 4, 4), &color, 255);

        assert_eq!(pixel_alpha(&buf, 4, 0, 0), 255, "0xF -> opaque");
        assert_eq!(pixel_alpha(&buf, 4, 1, 0), 0, "0x0 -> blank");
    }

    #[test]
    fn coverage_mid_value_scales_to_alpha() {
        let coverage = [0x80_u8];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_coverage_glyph(&coverage, 4, 1, 1, 0, 0, (0, 0, 4, 4), &color, 255);

        let r = buf[0];
        assert!((130..=140).contains(&r), "0x8/0xF * 255 ≈ 136, got {r}");
    }

    #[test]
    fn coverage_respects_clip_bounds() {
        let coverage = [0xFF_u8, 0xFF];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_coverage_glyph(&coverage, 4, 4, 1, 0, 0, (0, 0, 2, 4), &color, 255);

        assert_eq!(pixel_alpha(&buf, 4, 0, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 1, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 2, 0), 0, "clipped at x=2");
    }

    #[test]
    fn coverage_rows_use_minimum_byte_stride() {
        let coverage = [0xF0_u8, 0xF0, 0x0F, 0x00];
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_coverage_glyph(&coverage, 4, 3, 2, 0, 0, (0, 0, 4, 4), &color, 255);

        assert_eq!(pixel_alpha(&buf, 4, 0, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 1, 0), 0);
        assert_eq!(pixel_alpha(&buf, 4, 2, 0), 255);
        assert_eq!(pixel_alpha(&buf, 4, 0, 1), 0);
        assert_eq!(pixel_alpha(&buf, 4, 1, 1), 255);
        assert_eq!(pixel_alpha(&buf, 4, 2, 1), 0);
    }

    #[test]
    fn coverage_resamples_to_the_physical_extent() {
        let coverage = [0x80_u8];
        let region = mirx::image::Region::new(0, 0, 1, 1).unwrap();
        let mut buf = vec![0u8; 4 * 4 * 4];
        let tex = Texture::new(&mut buf, 4, 4, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let color = Color::rgba(255, 255, 255, 255);

        backend.blit_coverage_region(
            &coverage,
            1,
            region,
            1,
            0,
            0,
            2,
            3,
            (0, 0, 4, 4),
            &color,
            255,
        );

        for y in 0..3 {
            for x in 0..2 {
                assert_eq!(pixel_alpha(&buf, 4, x, y), 255);
            }
        }
        assert_eq!(pixel_alpha(&buf, 4, 2, 0), 0);
        assert_eq!(pixel_alpha(&buf, 4, 0, 3), 0);
    }

    #[test]
    fn coverage_scaling_interpolates_between_texels() {
        let coverage = [0_u8, 255];
        let region = mirx::image::Region::new(0, 0, 2, 1).unwrap();
        let mut buf = vec![0u8; 4 * 4];
        let tex = Texture::new(&mut buf, 4, 1, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        backend.blit_coverage_region(
            &coverage,
            2,
            region,
            8,
            0,
            0,
            4,
            1,
            (0, 0, 4, 1),
            &Color::rgba(255, 255, 255, 255),
            255,
        );

        assert_eq!(buf[0], 0);
        assert!((60..=65).contains(&buf[4]));
        assert!((190..=192).contains(&buf[8]));
        assert_eq!(buf[12], 255);
    }

    struct RecordingProvider {
        glyph_size: Rc<Cell<u16>>,
        metric_size: Rc<Cell<u16>>,
    }

    impl FontProvider for RecordingProvider {
        fn face_id(&self) -> FontFaceId {
            FontFaceId::new(2)
        }

        fn map_char(&self, _ch: char) -> Option<GlyphId> {
            Some(GlyphId::new(1))
        }

        fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
            Some(Fixed::from_int(4))
        }

        fn raster(
            &self,
            _glyph: GlyphId,
            layout_ppem: u16,
            output_ppem: u16,
        ) -> Option<RasterGlyph<'_>> {
            self.glyph_size.set(output_ppem);
            assert_eq!(layout_ppem, 16);
            Some(RasterGlyph {
                surface: GlyphSurface::new(
                    &[],
                    0,
                    0,
                    0,
                    mirx::image::SampleLayout::A1,
                    mirx::types::ByteAlignment::ONE,
                    FontSurfaceId::new(2),
                )
                .unwrap(),
                region: None,
                representation: mirx::font::FontRepresentation::coverage(1, 16, 0).unwrap(),
                offset_x: Fixed::from_ratio(-1, 2),
                offset_y: Fixed::from_ratio(1, 4),
            })
        }

        fn metrics(&self, requested_size: u16) -> FontMetrics {
            self.metric_size.set(requested_size);
            FontMetrics {
                ascender: Fixed::from_int(12),
                descender: Fixed::from_int(-4),
                line_height: Fixed::from_int(16),
            }
        }
    }

    #[test]
    fn viewport_scale_changes_only_output_representation_selection() {
        let glyph_size = Rc::new(Cell::new(0));
        let metric_size = Rc::new(Cell::new(0));
        let provider = RecordingProvider {
            glyph_size: glyph_size.clone(),
            metric_size: metric_size.clone(),
        };
        let font = Font {
            family: "recording",
            size: 16,
            backend: FontBackend::Custom(Rc::new(provider)),
        };
        let mut buf = vec![0u8; 64 * 64 * 4];
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        backend.viewport = Viewport::new(64, 64, Fixed::from_int(2));

        let glyphs = [textflow::shaping::PositionedGlyph::new(
            GlyphId::new(1),
            textflow::shaping::FlowPoint { x: 0, y: 9 << 8 },
        )];
        backend.draw_glyph_run_inner(
            &Point::ZERO,
            &glyphs,
            &font,
            &Rect::new(0, 0, 32, 32),
            &Color::rgba(255, 255, 255, 255),
            255,
        );

        assert_eq!(metric_size.get(), 16);
        assert_eq!(glyph_size.get(), 32);
        assert!(buf.iter().all(|byte| *byte == 0));
    }

    struct SizedRasterProvider;

    impl FontProvider for SizedRasterProvider {
        fn face_id(&self) -> FontFaceId {
            FontFaceId::new(3)
        }

        fn map_char(&self, _ch: char) -> Option<GlyphId> {
            Some(GlyphId::new(1))
        }

        fn glyph_advance(&self, _glyph: GlyphId, _ppem: u16) -> Option<Fixed> {
            Some(Fixed::from_int(4))
        }

        fn raster(
            &self,
            _glyph: GlyphId,
            layout_ppem: u16,
            output_ppem: u16,
        ) -> Option<RasterGlyph<'_>> {
            assert_eq!(layout_ppem, 8);
            assert_eq!(output_ppem, 8);
            Some(RasterGlyph {
                surface: GlyphSurface::new(
                    &[0x80],
                    1,
                    1,
                    1,
                    mirx::image::SampleLayout::A1,
                    mirx::types::ByteAlignment::ONE,
                    FontSurfaceId::new(3),
                )
                .unwrap(),
                region: Some(mirx::image::Region::new(0, 0, 1, 1).unwrap()),
                representation: mirx::font::FontRepresentation::coverage(1, 16, 0).unwrap(),
                offset_x: Fixed::ZERO,
                offset_y: Fixed::ZERO,
            })
        }

        fn metrics(&self, requested_size: u16) -> FontMetrics {
            assert_eq!(requested_size, 8);
            FontMetrics {
                ascender: Fixed::ZERO,
                descender: Fixed::ZERO,
                line_height: Fixed::from_int(8),
            }
        }
    }

    #[test]
    fn target_size_advance_is_not_scaled_by_the_raster_representation() {
        let font = Font {
            family: "sized-raster",
            size: 8,
            backend: FontBackend::Custom(Rc::new(SizedRasterProvider)),
        };
        let mut buf = vec![0u8; 12 * 2 * 4];
        let tex = Texture::new(&mut buf, 12, 2, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);

        let glyphs = [
            textflow::shaping::PositionedGlyph::new(
                GlyphId::new(1),
                textflow::shaping::FlowPoint { x: 0, y: 0 },
            ),
            textflow::shaping::PositionedGlyph::new(
                GlyphId::new(1),
                textflow::shaping::FlowPoint { x: 4 << 8, y: 0 },
            ),
        ];
        backend.draw_glyph_run_inner(
            &Point::ZERO,
            &glyphs,
            &font,
            &Rect::new(0, 0, 12, 2),
            &Color::rgba(255, 255, 255, 255),
            255,
        );

        assert_eq!(pixel_alpha(&buf, 12, 0, 0), 255);
        assert_eq!(pixel_alpha(&buf, 12, 4, 0), 255);
        assert_eq!(pixel_alpha(&buf, 12, 2, 0), 0);
    }
}
