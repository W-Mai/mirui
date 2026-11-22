use super::SwRenderer;
use crate::render::canvas::Paint;
use crate::render::paint::{GradientPaint, ProjectiveGradientPaint};
use crate::render::path::{self, Path, PathCmd};
use crate::render::raster::{self, FillRule};
use crate::render::renderer::ProjectiveDrawError;
use crate::types::{Color, Fixed, Rect, Transform, Transform3D};

#[inline]
pub(super) fn solid_alpha(opacity: u8, alpha: u8) -> Fixed {
    let opacity =
        Fixed::from_int(i32::from(opacity)).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
    let alpha = Fixed::from_int(i32::from(alpha)).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
    opacity * alpha
}

fn stroked_paint_bbox(outline: &Path, physical: &Transform) -> Option<Rect> {
    let inverse = physical.inverse()?;
    path::bbox_of_cmds_transformed(&outline.cmds, Some(&inverse))
}

fn segment_bounds(segments: &[raster::LineSeg]) -> Option<Rect> {
    let first = segments.first()?;
    let mut min_x = first.p1.x.min(first.p2.x);
    let mut min_y = first.p1.y.min(first.p2.y);
    let mut max_x = first.p1.x.max(first.p2.x);
    let mut max_y = first.p1.y.max(first.p2.y);
    for segment in &segments[1..] {
        min_x = min_x.min(segment.p1.x).min(segment.p2.x);
        min_y = min_y.min(segment.p1.y).min(segment.p2.y);
        max_x = max_x.max(segment.p1.x).max(segment.p2.x);
        max_y = max_y.max(segment.p1.y).max(segment.p2.y);
    }
    Some(Rect {
        x: min_x,
        y: min_y,
        w: max_x - min_x,
        h: max_y - min_y,
    })
}

struct ProjectivePathSpec<'a> {
    geometry: &'a Transform3D,
    paint_projection: Transform3D,
    paint_bbox: Rect,
    clip: Rect,
    paint: &'a Paint,
    opacity: u8,
    fill_rule: FillRule,
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sw/path/clip_tests.rs"]
mod clip_tests;

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sw/path/linear_transform_tests.rs"]
mod linear_transform_tests;

impl SwRenderer<'_> {
    pub(super) fn push_clip_inner(
        &mut self,
        path: &Path,
        phys_tf: &Transform,
        fill_rule: FillRule,
    ) {
        let w = self.target.width as usize;
        let h = self.target.height as usize;
        let scratch = &mut *self.scratch;
        if scratch.clip_mask_buf.capacity() < w * h {
            if let Some(index) = scratch
                .clip_recycled
                .iter()
                .position(|mask| mask.alpha.capacity() >= w * h)
            {
                let mut mask = scratch.clip_recycled.swap_remove(index);
                core::mem::swap(&mut scratch.clip_mask_buf, &mut mask.alpha);
                if mask.alpha.capacity() > 0 {
                    scratch.clip_recycled.push(mask);
                }
            }
        }
        scratch.clip_mask_buf.clear();
        scratch.clip_mask_buf.resize(w * h, 0);

        raster::flatten_into(&path.cmds, Some(phys_tf), &mut scratch.flatten_buf);
        if !scratch.flatten_buf.is_empty() {
            let screen = Rect::new(0, 0, self.target.width, self.target.height);
            if let Some(bbox) = path::bbox_of_cmds_transformed(&path.cmds, Some(phys_tf)) {
                if let Some(draw_area) = bbox.intersect(&screen) {
                    let (px_x0, px_y0, px_x1, py_y1) = draw_area.pixel_bounds();
                    let segs = &scratch.flatten_buf;
                    let acc = &mut scratch.scanline_acc;
                    let crossings = &mut scratch.scanline_crossings;
                    let mask = &mut scratch.clip_mask_buf;
                    raster::scanline_fill(
                        segs,
                        px_x0,
                        px_y0,
                        px_x1,
                        py_y1,
                        fill_rule,
                        acc,
                        crossings,
                        |px, py, cov| {
                            let idx = py as usize * w + px as usize;
                            mask[idx] = cov.map01(255).to_int() as u8;
                        },
                    );
                }
            }
        }

        if let Some(prev) = scratch.clip_stack.last() {
            for (dst, prev) in scratch.clip_mask_buf.iter_mut().zip(prev.alpha.iter()) {
                *dst = (*dst).min(*prev);
            }
        }

        let alpha = core::mem::take(&mut scratch.clip_mask_buf);
        scratch.clip_stack.push(super::ClipMask { alpha });
    }

    pub(super) fn push_clip_projective(
        &mut self,
        path: &Path,
        physical: &Transform3D,
        fill_rule: FillRule,
    ) -> Result<(), ProjectiveDrawError> {
        let w = self.target.width as usize;
        let h = self.target.height as usize;
        let scratch = &mut *self.scratch;
        if scratch.clip_mask_buf.capacity() < w * h {
            if let Some(index) = scratch
                .clip_recycled
                .iter()
                .position(|mask| mask.alpha.capacity() >= w * h)
            {
                let mut mask = scratch.clip_recycled.swap_remove(index);
                core::mem::swap(&mut scratch.clip_mask_buf, &mut mask.alpha);
                if mask.alpha.capacity() > 0 {
                    scratch.clip_recycled.push(mask);
                }
            }
        }
        scratch.clip_mask_buf.clear();
        scratch.clip_mask_buf.resize(w * h, 0);
        raster::flatten_projective_into(&path.cmds, physical, &mut scratch.flatten_buf)
            .map_err(|()| ProjectiveDrawError::InvalidProjection)?;
        if let Some(bounds) = segment_bounds(&scratch.flatten_buf) {
            let screen = Rect::new(0, 0, self.target.width, self.target.height);
            if let Some(draw_area) = bounds.inflate(Fixed::ONE).intersect(&screen) {
                let (px_x0, px_y0, px_x1, py_y1) = draw_area.pixel_bounds();
                let segments = &scratch.flatten_buf;
                let acc = &mut scratch.scanline_acc;
                let crossings = &mut scratch.scanline_crossings;
                let mask = &mut scratch.clip_mask_buf;
                raster::scanline_fill(
                    segments,
                    px_x0,
                    px_y0,
                    px_x1,
                    py_y1,
                    fill_rule,
                    acc,
                    crossings,
                    |px, py, coverage| {
                        mask[py as usize * w + px as usize] = coverage.map01(255).to_int() as u8;
                    },
                );
            }
        }
        if let Some(previous) = scratch.clip_stack.last() {
            for (next, previous) in scratch.clip_mask_buf.iter_mut().zip(previous.alpha.iter()) {
                *next = (*next).min(*previous);
            }
        }
        let alpha = core::mem::take(&mut scratch.clip_mask_buf);
        scratch.clip_stack.push(super::ClipMask { alpha });
        Ok(())
    }

    pub(super) fn fill_path_projective(
        &mut self,
        path: &Path,
        physical: &Transform3D,
        clip: Rect,
        paint: &Paint,
        opa: u8,
        fill_rule: FillRule,
    ) -> Result<(), ProjectiveDrawError> {
        self.fill_commands_projective(
            path.commands(),
            ProjectivePathSpec {
                geometry: physical,
                paint_projection: *physical,
                paint_bbox: path.bbox().unwrap_or(Rect::ZERO),
                clip,
                paint,
                opacity: opa,
                fill_rule,
            },
        )
    }

    fn fill_commands_projective(
        &mut self,
        commands: &[PathCmd],
        spec: ProjectivePathSpec<'_>,
    ) -> Result<(), ProjectiveDrawError> {
        if spec.opacity == 0 {
            return Ok(());
        }
        let gradient = match spec.paint {
            Paint::LinearGradient(_) | Paint::RadialGradient(_) => Some(
                ProjectiveGradientPaint::new(spec.paint, spec.paint_projection, spec.paint_bbox)
                    .ok_or(ProjectiveDrawError::InvalidProjection)?,
            ),
            Paint::Color(_) => None,
        };
        let solid_color = match spec.paint {
            Paint::Color(color) => (*color).into(),
            _ => Color::rgba(255, 255, 255, 255),
        };
        let scratch = &mut *self.scratch;
        raster::flatten_projective_into(commands, spec.geometry, &mut scratch.flatten_buf)
            .map_err(|()| ProjectiveDrawError::InvalidProjection)?;
        let Some(bounds) = segment_bounds(&scratch.flatten_buf) else {
            return Ok(());
        };
        let screen = Rect::new(0, 0, self.target.width, self.target.height);
        let Some(draw_area) = bounds
            .inflate(Fixed::ONE)
            .intersect(&spec.clip)
            .and_then(|bounds| bounds.intersect(&screen))
        else {
            return Ok(());
        };
        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let opacity =
            Fixed::from_int(spec.opacity as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let segments = &scratch.flatten_buf;
        let target_width = self.target.width as usize;
        let clip_mask = scratch.clip_stack.last().map(|mask| mask.alpha.as_slice());
        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return Ok(());
        };
        let format = target.format();
        let target_stride = target.stride();
        let pixel_size = format.bytes_per_pixel();
        let alpha_mode = target.alpha_mode();
        let target_bytes = target.bytes_mut();
        let acc = &mut scratch.scanline_acc;
        let crossings = &mut scratch.scanline_crossings;
        raster::scanline_fill(
            segments,
            px_x0,
            px_y0,
            px_x1,
            px_y1,
            spec.fill_rule,
            acc,
            crossings,
            |px, py, coverage| {
                let coverage = (coverage * opacity).map01(255).to_int() as u8;
                let clip_alpha = clip_mask
                    .map(|mask| mask[py as usize * target_width + px as usize])
                    .unwrap_or(255);
                let color = gradient
                    .as_ref()
                    .map(|gradient| gradient.sample(px, py))
                    .unwrap_or(solid_color);
                let final_alpha =
                    ((u32::from(coverage) * u32::from(clip_alpha) * u32::from(color.a) + 32_512)
                        / 65_025) as u8;
                if final_alpha != 0 {
                    format.blend_pixel(
                        target_bytes,
                        py as usize * target_stride + px as usize * pixel_size,
                        &color,
                        final_alpha,
                        alpha_mode,
                    );
                }
            },
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn stroke_commands_projective(
        &mut self,
        commands: &[PathCmd],
        geometry_transform: &Transform,
        physical_projective: &Transform3D,
        clip: Rect,
        width: Fixed,
        paint: &Paint,
        opa: u8,
        cap: crate::render::raster::LineCap,
        join: crate::render::raster::LineJoin,
        miter_limit: Fixed,
        dash: &[Fixed],
    ) -> Result<(), ProjectiveDrawError> {
        if opa == 0 || width <= Fixed::ZERO {
            return Ok(());
        }
        {
            let scratch = &mut *self.scratch;
            raster::offset_polygon_into(
                commands,
                Some(geometry_transform),
                width,
                cap,
                join,
                miter_limit,
                if dash.is_empty() { None } else { Some(dash) },
                &mut scratch.stroke_outline,
                &mut scratch.flatten_buf,
                &mut scratch.subpath_scratch,
                &mut scratch.semantic_joins,
                &mut scratch.stroke_normals,
                &mut scratch.stroke_rail,
                &mut scratch.stroke_left_rail,
                &mut scratch.stroke_arc,
                &mut scratch.dash_segments,
                &mut scratch.dash_scratch,
                &mut scratch.dashed_semantic_joins,
            );
        }
        let outline = core::mem::take(&mut self.scratch.stroke_outline);
        let paint_bbox = if !matches!(paint, Paint::Color(_)) {
            stroked_paint_bbox(&outline, geometry_transform).unwrap_or(Rect::ZERO)
        } else {
            Rect::ZERO
        };
        let paint_projection =
            physical_projective.compose(&Transform3D::from_affine(*geometry_transform));
        let result = self.fill_commands_projective(
            outline.commands(),
            ProjectivePathSpec {
                geometry: physical_projective,
                paint_projection,
                paint_bbox,
                clip,
                paint,
                opacity: opa,
                fill_rule: FillRule::EvenOdd,
            },
        );
        self.scratch.stroke_outline = outline;
        result
    }

    pub(super) fn fill_path_inner(
        &mut self,
        path: &Path,
        clip: &Rect,
        paint: &Paint,
        opa: u8,
        fill_rule: FillRule,
    ) {
        if opa == 0 {
            return;
        }
        let phys_tf = self.viewport.as_transform();
        let phys_clip = self.physical_draw_clip(*clip);
        self.fill_path_transformed(path, phys_clip, &phys_tf, paint, opa, fill_rule);
    }

    pub(super) fn fill_path_transformed(
        &mut self,
        path: &Path,
        phys_clip: Rect,
        phys_tf: &Transform,
        paint: &Paint,
        opa: u8,
        fill_rule: FillRule,
    ) {
        if opa == 0 {
            return;
        }
        let mut gradient = match paint {
            Paint::LinearGradient(_) | Paint::RadialGradient(_) => {
                let Some(bbox) = path.bbox() else { return };
                let Some(sampler) = GradientPaint::new(paint, *phys_tf, bbox) else {
                    return;
                };
                Some(sampler)
            }
            _ => None,
        };
        let scratch = &mut *self.scratch;
        raster::flatten_into(&path.cmds, Some(phys_tf), &mut scratch.flatten_buf);
        if scratch.flatten_buf.is_empty() {
            return;
        }
        // PathCmd bbox keeps the AA edge pixels at curve extrema that
        // a tight LineSeg hull would clip.
        let Some(bbox) = path::bbox_of_cmds_transformed(&path.cmds, Some(phys_tf)) else {
            return;
        };
        let screen = Rect::new(0, 0, self.target.width, self.target.height);
        let Some(draw_area) = bbox
            .intersect(&phys_clip)
            .and_then(|r| r.intersect(&screen))
        else {
            return;
        };

        let (px_x0, px_y0, px_x1, px_y1) = draw_area.pixel_bounds();
        let solid_color = match paint {
            Paint::Color(color) => (*color).into(),
            _ => Color::rgba(255, 255, 255, 255),
        };
        let combined_alpha = solid_alpha(opa, solid_color.a);

        let segs = &scratch.flatten_buf;
        let target_w = self.target.width as usize;
        let clip_mask = scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        let format = target.format();
        let target_stride = target.stride();
        let pixel_size = format.bytes_per_pixel();
        let alpha_mode = target.alpha_mode();
        let target_bytes = target.bytes_mut();
        let acc = &mut scratch.scanline_acc;
        let crossings = &mut scratch.scanline_crossings;
        if format.can_batch_solid(alpha_mode)
            && clip_mask.is_none()
            && gradient
                .as_ref()
                .is_none_or(GradientPaint::has_constant_rows)
        {
            let is_gradient = gradient.is_some();
            raster::scanline_fill_rows(
                segs,
                px_x0,
                px_y0,
                px_x1,
                px_y1,
                fill_rule,
                acc,
                crossings,
                |x, py, differences| {
                    let begin = py as usize * target_stride + x as usize * pixel_size;
                    let end = begin + differences.len() * pixel_size;
                    let target_row = &mut target_bytes[begin..end];
                    let color = gradient
                        .as_mut()
                        .map(|paint| paint.sample(x, py))
                        .unwrap_or(solid_color);
                    raster::coverage_spans(differences, |start, end, cov| {
                        let base_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
                        let alpha = if is_gradient {
                            ((u16::from(base_alpha) * u16::from(color.a) + 127) / 255) as u8
                        } else {
                            base_alpha
                        };
                        format.blend_solid_span(
                            &mut target_row[start * pixel_size..end * pixel_size],
                            &color,
                            alpha,
                            alpha_mode,
                        );
                    });
                },
            );
            return;
        }
        raster::scanline_fill(
            segs,
            px_x0,
            px_y0,
            px_x1,
            px_y1,
            fill_rule,
            acc,
            crossings,
            |px, py, cov| {
                let base_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
                let clip_alpha = clip_mask
                    .map(|m| m[py as usize * target_w + px as usize])
                    .unwrap_or(255);
                let final_alpha = ((base_alpha as u16 * clip_alpha as u16 + 127) / 255) as u8;
                if final_alpha > 0 {
                    if let Some(gradient) = gradient.as_mut() {
                        let c = gradient.sample(px, py);
                        let paint_alpha =
                            ((u16::from(final_alpha) * u16::from(c.a) + 127) / 255) as u8;
                        if paint_alpha != 0 {
                            format.blend_pixel(
                                target_bytes,
                                py as usize * target_stride + px as usize * pixel_size,
                                &c,
                                paint_alpha,
                                alpha_mode,
                            );
                        }
                    } else {
                        format.blend_pixel(
                            target_bytes,
                            py as usize * target_stride + px as usize * pixel_size,
                            &solid_color,
                            final_alpha,
                            alpha_mode,
                        );
                    }
                }
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn stroke_path_inner(
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
        self.stroke_commands_inner(
            path.commands(),
            clip,
            width,
            paint,
            opa,
            cap,
            join,
            miter_limit,
            dash,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn stroke_commands_inner(
        &mut self,
        commands: &[PathCmd],
        clip: &Rect,
        width: Fixed,
        paint: &Paint,
        opa: u8,
        cap: crate::render::raster::LineCap,
        join: crate::render::raster::LineJoin,
        miter_limit: Fixed,
        dash: &[Fixed],
    ) {
        if opa == 0 || width <= Fixed::ZERO {
            return;
        }
        let phys_tf = self.viewport.as_transform();
        let phys_width = width * self.viewport.scale();
        {
            let scratch = &mut *self.scratch;
            raster::offset_polygon_into(
                commands,
                Some(&phys_tf),
                phys_width,
                cap,
                join,
                miter_limit,
                if dash.is_empty() { None } else { Some(dash) },
                &mut scratch.stroke_outline,
                &mut scratch.flatten_buf,
                &mut scratch.subpath_scratch,
                &mut scratch.semantic_joins,
                &mut scratch.stroke_normals,
                &mut scratch.stroke_rail,
                &mut scratch.stroke_left_rail,
                &mut scratch.stroke_arc,
                &mut scratch.dash_segments,
                &mut scratch.dash_scratch,
                &mut scratch.dashed_semantic_joins,
            );
        }
        let outline_cmds = core::mem::take(&mut self.scratch.stroke_outline);
        let phys_clip = self.physical_draw_clip(*clip);
        let paint_bbox = if !matches!(paint, Paint::Color(_)) {
            stroked_paint_bbox(&outline_cmds, &phys_tf).unwrap_or(Rect::new(0, 0, 0, 0))
        } else {
            Rect::new(0, 0, 0, 0)
        };
        self.fill_physical_path_with_paint(
            &outline_cmds,
            &phys_clip,
            paint,
            opa,
            &phys_tf,
            paint_bbox,
        );
        self.scratch.stroke_outline = outline_cmds;
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn stroke_path_transformed(
        &mut self,
        path: &Path,
        phys_clip: Rect,
        phys_tf: &Transform,
        width: Fixed,
        paint: &Paint,
        opa: u8,
        cap: crate::render::raster::LineCap,
        join: crate::render::raster::LineJoin,
        miter_limit: Fixed,
        dash: &[Fixed],
    ) {
        if opa == 0 || width <= Fixed::ZERO {
            return;
        }
        let phys_width = width * self.viewport.scale();
        {
            let scratch = &mut *self.scratch;
            raster::offset_polygon_into(
                &path.cmds,
                Some(phys_tf),
                phys_width,
                cap,
                join,
                miter_limit,
                if dash.is_empty() { None } else { Some(dash) },
                &mut scratch.stroke_outline,
                &mut scratch.flatten_buf,
                &mut scratch.subpath_scratch,
                &mut scratch.semantic_joins,
                &mut scratch.stroke_normals,
                &mut scratch.stroke_rail,
                &mut scratch.stroke_left_rail,
                &mut scratch.stroke_arc,
                &mut scratch.dash_segments,
                &mut scratch.dash_scratch,
                &mut scratch.dashed_semantic_joins,
            );
        }
        let outline_cmds = core::mem::take(&mut self.scratch.stroke_outline);
        let paint_bbox = if !matches!(paint, Paint::Color(_)) {
            stroked_paint_bbox(&outline_cmds, phys_tf).unwrap_or(Rect::new(0, 0, 0, 0))
        } else {
            Rect::new(0, 0, 0, 0)
        };
        self.fill_physical_path_with_paint(
            &outline_cmds,
            &phys_clip,
            paint,
            opa,
            phys_tf,
            paint_bbox,
        );
        self.scratch.stroke_outline = outline_cmds;
    }

    pub(super) fn fill_physical_path_with_paint(
        &mut self,
        phys_path: &Path,
        phys_clip: &Rect,
        paint: &Paint,
        opa: u8,
        paint_tf: &Transform,
        paint_bbox: Rect,
    ) {
        self.fill_physical_path(phys_path, phys_clip, paint, opa, paint_tf, paint_bbox);
    }

    pub(super) fn fill_physical_path(
        &mut self,
        phys_path: &Path,
        phys_clip: &Rect,
        paint: &Paint,
        opa: u8,
        paint_tf: &Transform,
        paint_bbox: Rect,
    ) {
        if opa == 0 {
            return;
        }
        let mut gradient = match paint {
            Paint::LinearGradient(_) | Paint::RadialGradient(_) => {
                let Some(sampler) = GradientPaint::new(paint, *paint_tf, paint_bbox) else {
                    return;
                };
                Some(sampler)
            }
            _ => None,
        };
        let scratch = &mut *self.scratch;
        raster::flatten_into(&phys_path.cmds, None, &mut scratch.flatten_buf);
        if scratch.flatten_buf.is_empty() {
            return;
        }
        let Some(bbox) = phys_path.bbox() else { return };
        let screen = Rect::new(0, 0, self.target.width, self.target.height);
        let Some(draw_area) = bbox.intersect(phys_clip).and_then(|r| r.intersect(&screen)) else {
            return;
        };

        let (px_x0, px_y0, px_x1, py_y1) = draw_area.pixel_bounds();
        let solid_color = match paint {
            Paint::Color(color) => (*color).into(),
            _ => Color::rgba(255, 255, 255, 255),
        };
        let combined_alpha = solid_alpha(opa, solid_color.a);

        let segs = &scratch.flatten_buf;
        let target_w = self.target.width as usize;
        let clip_mask = scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let Some(mut target) = super::write_pixels(&mut self.target, &mut self.failure) else {
            return;
        };
        let format = target.format();
        let target_stride = target.stride();
        let pixel_size = format.bytes_per_pixel();
        let alpha_mode = target.alpha_mode();
        let target_bytes = target.bytes_mut();
        let acc = &mut scratch.scanline_acc;
        let crossings = &mut scratch.scanline_crossings;
        if format.can_batch_solid(alpha_mode)
            && clip_mask.is_none()
            && gradient
                .as_ref()
                .is_none_or(GradientPaint::has_constant_rows)
        {
            let is_gradient = gradient.is_some();
            raster::scanline_fill_rows(
                segs,
                px_x0,
                px_y0,
                px_x1,
                py_y1,
                FillRule::EvenOdd,
                acc,
                crossings,
                |x, py, differences| {
                    let begin = py as usize * target_stride + x as usize * pixel_size;
                    let end = begin + differences.len() * pixel_size;
                    let target_row = &mut target_bytes[begin..end];
                    let color = gradient
                        .as_mut()
                        .map(|paint| paint.sample(x, py))
                        .unwrap_or(solid_color);
                    raster::coverage_spans(differences, |start, end, cov| {
                        let base_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
                        let alpha = if is_gradient {
                            ((u16::from(base_alpha) * u16::from(color.a) + 127) / 255) as u8
                        } else {
                            base_alpha
                        };
                        format.blend_solid_span(
                            &mut target_row[start * pixel_size..end * pixel_size],
                            &color,
                            alpha,
                            alpha_mode,
                        );
                    });
                },
            );
            return;
        }
        raster::scanline_fill(
            segs,
            px_x0,
            px_y0,
            px_x1,
            py_y1,
            FillRule::EvenOdd,
            acc,
            crossings,
            |px, py, cov| {
                let base_alpha = (cov * combined_alpha).map01(255).to_int() as u8;
                let clip_alpha = clip_mask
                    .map(|m| m[py as usize * target_w + px as usize])
                    .unwrap_or(255);
                let final_alpha = ((base_alpha as u16 * clip_alpha as u16 + 127) / 255) as u8;
                if final_alpha > 0 {
                    if let Some(gradient) = gradient.as_mut() {
                        let c = gradient.sample(px, py);
                        let paint_alpha =
                            ((u16::from(final_alpha) * u16::from(c.a) + 127) / 255) as u8;
                        if paint_alpha != 0 {
                            format.blend_pixel(
                                target_bytes,
                                py as usize * target_stride + px as usize * pixel_size,
                                &c,
                                paint_alpha,
                                alpha_mode,
                            );
                        }
                    } else {
                        format.blend_pixel(
                            target_bytes,
                            py as usize * target_stride + px as usize * pixel_size,
                            &solid_color,
                            final_alpha,
                            alpha_mode,
                        );
                    }
                }
            },
        );
    }
}

#[cfg(all(test, feature = "std"))]
#[path = "../../../../tests/unit/render/backends/sw/path/gradient_tests.rs"]
mod gradient_tests;
