use super::SwRenderer;
use crate::render::font::scalar::ScalarField;
use crate::render::font::sdf::SignedDistanceField;
use crate::render::font::{Font, Glyph, GlyphKind, raster_glyph_origin, raster_glyph_rect};
use crate::types::{Color, Fixed, Fixed64, Point, Rect, Transform, Transform3D, fixed::storage};

#[derive(Clone, Copy)]
struct GlyphRasterContext {
    requested_size: u16,
    viewport_scale: Fixed,
    bounds: (i32, i32, i32, i32),
}

#[derive(Clone, Copy)]
struct RasterInverse {
    m00: Fixed64,
    m01: Fixed64,
    tx: Fixed64,
    m10: Fixed64,
    m11: Fixed64,
    ty: Fixed64,
}

impl RasterInverse {
    fn new(transform: &Transform) -> Option<Self> {
        let m00 = Fixed64::from(transform.m00);
        let m01 = Fixed64::from(transform.m01);
        let tx = Fixed64::from(transform.tx);
        let m10 = Fixed64::from(transform.m10);
        let m11 = Fixed64::from(transform.m11);
        let ty = Fixed64::from(transform.ty);
        let determinant = m00 * m11 - m01 * m10;
        if determinant.is_zero() {
            return None;
        }
        let inverse_determinant = Fixed64::ONE / determinant;
        Some(Self {
            m00: m11 * inverse_determinant,
            m01: -m01 * inverse_determinant,
            tx: (m01 * ty - m11 * tx) * inverse_determinant,
            m10: -m10 * inverse_determinant,
            m11: m00 * inverse_determinant,
            ty: (m10 * tx - m00 * ty) * inverse_determinant,
        })
    }

    fn apply(self, x: Fixed64, y: Fixed64) -> (Fixed64, Fixed64) {
        (
            self.m00 * x + self.m01 * y + self.tx,
            self.m10 * x + self.m11 * y + self.ty,
        )
    }
}

enum TransformedGlyph<'a> {
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
        let context = GlyphRasterContext {
            requested_size,
            viewport_scale,
            bounds: self.physical_draw_clip(*clip).pixel_bounds(),
        };
        for positioned in glyphs {
            let Some(glyph) =
                font.glyph_by_id_for_output(positioned.glyph_id(), requested_size, output_ppem)
            else {
                continue;
            };
            let Some((x, baseline)) = raster_glyph_origin(
                phys_pos,
                first.origin,
                positioned,
                metrics.ascender,
                viewport_scale,
            ) else {
                continue;
            };
            let Some(advance) = positioned
                .origin
                .x
                .checked_sub(first.origin.x)
                .and_then(|value| value.checked_add(positioned.offset.x))
            else {
                continue;
            };
            let exact_x = Fixed::from_int(phys_pos.x.floor().to_int())
                + crate::types::fixed::from_textflow(advance) * viewport_scale;
            self.draw_glyph_at(&glyph, (x, exact_x), baseline, context, color, opa);
        }
    }

    pub(super) fn draw_glyph_run_transformed_inner(&mut self, run: TransformedRun<'_>) {
        let (Some(first), Some(inverse)) = (run.glyphs.first(), RasterInverse::new(run.transform))
        else {
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
        let output_ppem =
            crate::render::font::output_ppem(requested_size, run.transform.raster_scale());
        for (positioned, frame) in run.glyphs.iter().zip(run.frames) {
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
            if quad
                .transform
                .apply_rect_bbox(quad.rect)
                .intersect(&run.clip)
                .and_then(|area| {
                    area.intersect(&Rect::new(0, 0, self.target.width, self.target.height))
                })
                .is_none()
            {
                continue;
            }
            let Some(region) = raster.region else {
                continue;
            };
            if run.transform.is_identity()
                && raster.representation.design_ppem() == requested_size
                && let mirx::font::FontRepresentationKind::Coverage { bits } =
                    raster.representation.kind()
                && let Some(field) = ScalarField::new(
                    raster.surface.samples(),
                    raster.surface.stride(),
                    region,
                    bits,
                )
            {
                self.blit_posed_coverage_glyph(
                    field,
                    quad.rect,
                    Point {
                        x: quad.transform.tx,
                        y: quad.transform.ty,
                    },
                    Point {
                        x: quad.transform.m00,
                        y: quad.transform.m10,
                    },
                    run.clip,
                    run.color,
                    run.opacity,
                );
                continue;
            }
            let Some(inverse) = RasterInverse::new(&quad.transform) else {
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

    #[allow(clippy::too_many_arguments)]
    fn blit_posed_coverage_glyph(
        &mut self,
        field: ScalarField<'_>,
        logical_rect: Rect,
        origin: Point,
        tangent: Point,
        clip: Rect,
        color: &Color,
        opacity: u8,
    ) {
        let transform = Transform {
            m00: tangent.x,
            m01: -tangent.y,
            tx: origin.x,
            m10: tangent.y,
            m11: tangent.x,
            ty: origin.y,
        };
        let Some(draw_area) = transform
            .apply_rect_bbox(logical_rect)
            .intersect(&clip)
            .and_then(|area| {
                area.intersect(&Rect::new(0, 0, self.target.width, self.target.height))
            })
        else {
            return;
        };
        let clip_bounds = draw_area.pixel_bounds();
        let target_width = self.target.width as usize;
        let clip_mask = self
            .scratch
            .clip_stack
            .last()
            .map(|mask| mask.alpha.as_slice());
        let local_x = logical_rect.x + Fixed::HALF;
        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        for row in 0..field.height() {
            let local_y = logical_rect.y + Fixed::from_int(row as i32) + Fixed::HALF;
            let mut screen_x = origin.x + tangent.x * local_x - tangent.y * local_y;
            let mut screen_y = origin.y + tangent.y * local_x + tangent.x * local_y;
            for col in 0..field.width() {
                let coverage = field.alpha(col, row);
                if coverage != 0 {
                    splat_coverage_pixel(
                        &mut target,
                        screen_x - Fixed::HALF,
                        screen_y - Fixed::HALF,
                        clip_bounds,
                        target_width,
                        clip_mask,
                        color,
                        scale_opacity(opacity, coverage),
                    );
                }
                screen_x += tangent.x;
                screen_y += tangent.y;
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
            run.transform.raster_scale_at(*run.pos),
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
                run.projective_transform.raster_scale_at(*run.pos),
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
        let clip_mask = self
            .scratch
            .clip_stack
            .last()
            .map(|mask| mask.alpha.as_slice());
        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
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
                    target.blend_pixel_int(px, py, color, alpha);
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
        inverse: &RasterInverse,
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
        let source_scale_x =
            Fixed64::from_int(i64::from(source_width)) / Fixed64::from(logical_rect.w);
        let source_scale_y =
            Fixed64::from_int(i64::from(source_height)) / Fixed64::from(logical_rect.h);
        let source_dx_x = inverse.m00 * source_scale_x;
        let source_dx_y = inverse.m10 * source_scale_y;
        let source_dy_x = inverse.m01 * source_scale_x;
        let source_dy_y = inverse.m11 * source_scale_y;
        let source_dx_x_fixed = source_dx_x.to_fixed();
        let source_dx_y_fixed = source_dx_y.to_fixed();
        let source_dy_x_fixed = source_dy_x.to_fixed();
        let source_dy_y_fixed = source_dy_y.to_fixed();
        let logical_x = Fixed64::from(logical_rect.x);
        let logical_y = Fixed64::from(logical_rect.y);
        let source_w = Fixed64::from_int(i64::from(source_width));
        let source_h = Fixed64::from_int(i64::from(source_height));
        let half = Fixed64::from_ratio(1, 2);
        let (x0, y0, x1, y1) = draw_area.pixel_bounds();
        let target_width = self.target.width as usize;
        let clip_mask = self
            .scratch
            .clip_stack
            .last()
            .map(|mask| mask.alpha.as_slice());
        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        for py in y0..y1 {
            let mask_row = py as usize * target_width;
            let (sample_x, sample_y) = inverse.apply(
                Fixed64::from_int(i64::from(x0)) + half,
                Fixed64::from_int(i64::from(py)) + half,
            );
            let mut source_x = (sample_x - logical_x) * source_scale_x;
            let mut source_y = (sample_y - logical_y) * source_scale_y;
            for px in x0..x1 {
                let u = source_x;
                let v = source_y;
                source_x += source_dx_x;
                source_y += source_dx_y;
                if u < Fixed64::ZERO || v < Fixed64::ZERO || u >= source_w || v >= source_h {
                    continue;
                }
                let sx = (u - half).to_fixed();
                let sy = (v - half).to_fixed();
                let coverage = match &glyph {
                    TransformedGlyph::Coverage(field) => field.sample_bilinear(sx, sy),
                    TransformedGlyph::SignedDistance(field) => {
                        let (distance, gradient_x, gradient_y) = field.sample_with_gradient(sx, sy);
                        let screen_x =
                            gradient_x * source_dx_x_fixed + gradient_y * source_dx_y_fixed;
                        let screen_y =
                            gradient_x * source_dy_x_fixed + gradient_y * source_dy_y_fixed;
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
                    target.blend_pixel_int(px, py, color, alpha);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_glyph_at(
        &mut self,
        glyph: &Glyph<'_>,
        (x, exact_x): (i32, Fixed),
        baseline: i32,
        context: GlyphRasterContext,
        color: &Color,
        opa: u8,
    ) {
        match &glyph.kind {
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
                let texel = Fixed::from_int(i32::from(context.requested_size))
                    / Fixed::from_int(i32::from(representation.design_ppem().max(1)))
                    * context.viewport_scale;
                if texel != Fixed::ONE {
                    let field = match representation.kind() {
                        mirx::font::FontRepresentationKind::Coverage { bits } => {
                            ScalarField::new(samples, *stride, *region, bits)
                                .map(TransformedGlyph::Coverage)
                        }
                        mirx::font::FontRepresentationKind::SignedDistance { bits, spread } => {
                            SignedDistanceField::new(samples, *stride, *region, bits, spread)
                                .map(TransformedGlyph::SignedDistance)
                        }
                        _ => None,
                    };
                    if let Some(field) = field {
                        self.blit_scaled_glyph(
                            field,
                            Point {
                                x: exact_x + *bearing_x * context.viewport_scale,
                                y: Fixed::from_int(baseline) - *bearing_y * context.viewport_scale,
                            },
                            texel,
                            context.bounds,
                            color,
                            opa,
                        );
                    }
                    return;
                }
                let (raster_x, raster_y, width, height) = raster_glyph_rect(
                    (x, baseline),
                    Point {
                        x: *bearing_x,
                        y: *bearing_y,
                    },
                    *region,
                    context.requested_size,
                    representation.design_ppem(),
                    context.viewport_scale,
                );
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

    /// Paints a glyph drawn at a size other than its strike's. Each target
    /// pixel samples the raster at its exact position, so a scaled glyph keeps
    /// the width and spacing the layout gave it instead of growing to whole
    /// pixels. `origin` is the raster's top left in physical pixels and
    /// `texel` the physical size of one source sample.
    fn blit_scaled_glyph(
        &mut self,
        glyph: TransformedGlyph<'_>,
        origin: Point,
        texel: Fixed,
        bounds: (i32, i32, i32, i32),
        color: &Color,
        opa: u8,
    ) {
        if texel <= Fixed::ZERO {
            return;
        }
        let (source_width, source_height) = glyph.dimensions();
        let (clip_x, clip_y, clip_x2, clip_y2) = bounds;
        let x0 = origin.x.floor().to_int().max(clip_x).max(0);
        let y0 = origin.y.floor().to_int().max(clip_y).max(0);
        let x1 = (origin.x + Fixed::from_int(source_width as i32) * texel)
            .ceil()
            .to_int()
            .min(clip_x2)
            .min(self.target.width as i32);
        let y1 = (origin.y + Fixed::from_int(source_height as i32) * texel)
            .ceil()
            .to_int()
            .min(clip_y2)
            .min(self.target.height as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let step = Fixed::ONE / texel;
        let half = Fixed::HALF;
        let target_w = self.target.width as usize;
        let clip_mask = self.scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        for py in y0..y1 {
            let sy = (Fixed::from_int(py) + half - origin.y) * step - half;
            for px in x0..x1 {
                let sx = (Fixed::from_int(px) + half - origin.x) * step - half;
                let coverage = match &glyph {
                    // A smaller copy spreads thin stems over more pixels;
                    // lift the mid tones so they keep their weight.
                    TransformedGlyph::Coverage(field) if texel < Fixed::ONE => {
                        let value = padded_bilinear(field, sx, sy);
                        value + value * (Fixed::ONE - value) / 2
                    }
                    TransformedGlyph::Coverage(field) => padded_bilinear(field, sx, sy),
                    TransformedGlyph::SignedDistance(field) => {
                        let (distance, gradient_x, gradient_y) = field.sample_with_gradient(sx, sy);
                        let screen_dx = gradient_x * step;
                        let screen_dy = gradient_y * step;
                        let edge = ((screen_dx * screen_dx + screen_dy * screen_dy).sqrt() / 2)
                            .max(Fixed::from_ratio(1, 256));
                        if distance <= -edge {
                            Fixed::ZERO
                        } else if distance >= edge {
                            Fixed::ONE
                        } else {
                            (distance + edge) / (edge * 2)
                        }
                    }
                };
                let mut alpha = (coverage * Fixed::from_int(i32::from(opa)))
                    .to_int()
                    .clamp(0, 255) as u8;
                if alpha == 0 {
                    continue;
                }
                if let Some(mask) = clip_mask {
                    let ca = mask[py as usize * target_w + px as usize];
                    if ca == 0 {
                        continue;
                    }
                    if ca < 255 {
                        alpha = ((u16::from(alpha) * u16::from(ca) + 127) / 255) as u8;
                        if alpha == 0 {
                            continue;
                        }
                    }
                }
                target.blend_pixel_int(px, py, color, alpha);
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
        let exact_binary =
            bpp == 1 && source_width == target_width && source_height == target_height;
        let scale_x = Fixed::from_int(source_width as i32) / Fixed::from_int(target_width as i32);
        let scale_y = Fixed::from_int(source_height as i32) / Fixed::from_int(target_height as i32);
        let half_texel = Fixed::HALF;
        let target_w = self.target.width as usize;
        let clip_mask = self.scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        for row in 0..target_height {
            let source_y = (Fixed::from_int(row as i32) + half_texel) * scale_y - half_texel;
            for col in 0..target_width {
                let mut alpha = if exact_binary {
                    if field.alpha(col, row) == 0 {
                        continue;
                    }
                    base_opa
                } else {
                    let source_x =
                        (Fixed::from_int(col as i32) + half_texel) * scale_x - half_texel;
                    let coverage = field.sample_bilinear(source_x, source_y);
                    (coverage * Fixed::from_int(i32::from(base_opa)))
                        .to_int()
                        .clamp(0, 255) as u8
                };
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
                    target.blend_pixel_int(px, py, color, alpha);
                }
            }
        }
    }
}

/// Bilinear coverage that treats samples outside the raster as empty, so a
/// scaled glyph fades out at its edges instead of repeating its border.
fn padded_bilinear(field: &ScalarField<'_>, x: Fixed, y: Fixed) -> Fixed {
    let (width, height) = (field.width() as i32, field.height() as i32);
    let (left, top) = (x.floor(), y.floor());
    let (fx, fy) = (x - left, y - top);
    let (x0, y0) = (left.to_int(), top.to_int());
    let at = |sx: i32, sy: i32| {
        if sx < 0 || sy < 0 || sx >= width || sy >= height {
            Fixed::ZERO
        } else {
            field.sample(sx, sy)
        }
    };
    let upper = at(x0, y0) * (Fixed::ONE - fx) + at(x0 + 1, y0) * fx;
    let lower = at(x0, y0 + 1) * (Fixed::ONE - fx) + at(x0 + 1, y0 + 1) * fx;
    upper * (Fixed::ONE - fy) + lower * fy
}

fn scale_opacity(opacity: u8, coverage: u8) -> u8 {
    ((u16::from(opacity) * u16::from(coverage) + 127) / 255) as u8
}

#[allow(clippy::too_many_arguments)]
fn splat_coverage_pixel(
    target: &mut crate::render::texture::TexturePixelsMut<'_>,
    x: Fixed,
    y: Fixed,
    clip: (i32, i32, i32, i32),
    target_width: usize,
    clip_mask: Option<&[u8]>,
    color: &Color,
    opacity: u8,
) {
    let x0 = x.floor().to_int();
    let y0 = y.floor().to_int();
    let fx = storage::to_i32(x - Fixed::from_int(x0)) as u32;
    let fy = storage::to_i32(y - Fixed::from_int(y0)) as u32;
    let weights = [
        ((x0, y0), (256 - fx) * (256 - fy)),
        ((x0 + 1, y0), fx * (256 - fy)),
        ((x0, y0 + 1), (256 - fx) * fy),
        ((x0 + 1, y0 + 1), fx * fy),
    ];
    for ((px, py), weight) in weights {
        if weight == 0 || px < clip.0 || py < clip.1 || px >= clip.2 || py >= clip.3 {
            continue;
        }
        let mut alpha = ((u32::from(opacity) * weight + 32_768) >> 16) as u8;
        if let Some(mask) = clip_mask {
            alpha = ((u16::from(alpha) * u16::from(mask[py as usize * target_width + px as usize])
                + 127)
                / 255) as u8;
        }
        if alpha != 0 {
            target.blend_pixel_int(px, py, color, alpha);
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sw/label.rs"]
mod tests;
