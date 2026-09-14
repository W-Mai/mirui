use super::SwRenderer;
use crate::render::canvas::Paint;
use crate::render::paint::GradientPaint;
use crate::render::path::{self, Path};
use crate::render::raster::{self, FillRule};
use crate::types::{Color, Fixed, Rect, Transform};

fn stroked_paint_bbox(outline: &Path, physical: &Transform) -> Option<Rect> {
    let inverse = physical.inverse()?;
    path::bbox_of_cmds_transformed(&outline.cmds, Some(&inverse))
}

#[cfg(test)]
mod clip_tests {
    use super::*;
    use crate::render::raster::{LineCap, LineJoin};
    use crate::render::texture::{ColorFormat, Texture};
    use crate::types::{Point, Viewport};

    #[test]
    fn transformed_stroke_uses_physical_clip_once_at_hidpi_scale() {
        let mut renderer = SwRenderer::new(Texture::owned(64, 64, ColorFormat::RGBA8888));
        renderer.viewport = Viewport::new(64, 64, Fixed::from_int(2));
        let mut path = Path::new();
        path.move_to(Point::new(0, 20)).line_to(Point::new(40, 20));
        let paint = Paint::Color(Color::rgb(255, 0, 0).into());
        renderer.stroke_path_transformed(
            &path,
            Rect::new(0, 0, 16, 64),
            &Transform::IDENTITY,
            Fixed::from_int(2),
            &paint,
            255,
            LineCap::Butt,
            LineJoin::Miter,
            Fixed::from_int(4),
            &[],
        );
        assert_eq!(renderer.target.get_pixel(10, 20).r, 255);
        assert_eq!(renderer.target.get_pixel(24, 20).r, 0);
    }
}

#[cfg(test)]
mod linear_transform_tests {
    use super::*;
    use crate::render::command::DrawCommand;
    use crate::render::raster::{LineCap, LineJoin};
    use crate::render::renderer::{DrawRequest, RenderError, Renderer};
    use crate::render::texture::{ColorFormat, Texture};
    use alloc::borrow::Cow;
    use mirx::scene::{GradientStop, GradientUnits, LinearGradient, RadialGradient, SpreadMode};

    #[test]
    fn transformed_linear_fill_tracks_path_and_paint_transforms() {
        let mut renderer = SwRenderer::new(Texture::owned(220, 44, ColorFormat::RGBA8888));
        let path = Path::rect(0.into(), 0.into(), 100.into(), 20.into());
        let gradient = LinearGradient {
            start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
            end: mirx::types::Point::new(
                mirx::types::Fixed::from_int(100),
                mirx::types::Fixed::ZERO,
            ),
            stops: Cow::Owned(alloc::vec![
                GradientStop {
                    offset: mirx::types::Fixed::ZERO,
                    color: mirx::types::Color::rgb(0, 0, 0),
                },
                GradientStop {
                    offset: mirx::types::Fixed::ONE,
                    color: mirx::types::Color::rgb(255, 0, 0),
                },
            ]),
            spread: SpreadMode::Pad,
            units: GradientUnits::UserSpaceOnUse,
            transform: mirx::types::Transform::translate(
                mirx::types::Fixed::from_int(20),
                mirx::types::Fixed::ZERO,
            ),
        };
        renderer.fill_path_transformed(
            &path,
            Rect::new(0, 0, 220, 44),
            &Transform::scale(Fixed::from_int(2), Fixed::from_int(2)),
            &Paint::LinearGradient(gradient),
            255,
            FillRule::EvenOdd,
        );
        assert_eq!(renderer.target.get_pixel(20, 20).r, 0);
        assert!((126..=130).contains(&renderer.target.get_pixel(140, 20).r));
    }

    #[test]
    fn checked_linear_fill_rejects_empty_stops_before_drawing() {
        let mut renderer = SwRenderer::new(Texture::owned(32, 32, ColorFormat::RGBA8888));
        let path = Path::rect(0.into(), 0.into(), 20.into(), 20.into());
        let paint = Paint::LinearGradient(LinearGradient {
            start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
            end: mirx::types::Point::new(mirx::types::Fixed::ONE, mirx::types::Fixed::ZERO),
            stops: Cow::Borrowed(&[]),
            spread: SpreadMode::Pad,
            units: GradientUnits::ObjectBoundingBox,
            transform: mirx::types::Transform::IDENTITY,
        });
        let command = DrawCommand::FillPath {
            path: &path,
            transform: Transform::IDENTITY,
            paint: &paint,
            opa: 255,
            fill_rule: FillRule::EvenOdd,
        };
        let clip = Rect::new(0, 0, 32, 32);
        assert_eq!(
            renderer.submit(&DrawRequest::new(&command, clip)),
            Err(RenderError::InvalidGeometry)
        );
        assert_eq!(renderer.target.get_pixel(10, 10).r, 0);
    }

    #[test]
    fn linear_stop_alpha_modulates_path_coverage() {
        let mut renderer = SwRenderer::new(Texture::owned(16, 16, ColorFormat::RGBA8888));
        let path = Path::rect(0.into(), 0.into(), 16.into(), 16.into());
        let stop = mirx::types::Color::rgba(255, 0, 0, 128);
        let paint = Paint::LinearGradient(LinearGradient {
            start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
            end: mirx::types::Point::new(
                mirx::types::Fixed::from_int(16),
                mirx::types::Fixed::ZERO,
            ),
            stops: Cow::Owned(alloc::vec![
                GradientStop {
                    offset: mirx::types::Fixed::ZERO,
                    color: stop,
                },
                GradientStop {
                    offset: mirx::types::Fixed::ONE,
                    color: stop,
                },
            ]),
            spread: SpreadMode::Pad,
            units: GradientUnits::UserSpaceOnUse,
            transform: mirx::types::Transform::IDENTITY,
        });
        renderer.fill_path_transformed(
            &path,
            Rect::new(0, 0, 16, 16),
            &Transform::IDENTITY,
            &paint,
            255,
            FillRule::EvenOdd,
        );
        assert!((126..=129).contains(&renderer.target.get_pixel(8, 8).r));
    }

    #[test]
    fn object_bbox_linear_stroke_uses_outline_bounds_for_flat_path() {
        let mut renderer = SwRenderer::new(Texture::owned(48, 24, ColorFormat::RGBA8888));
        let mut path = Path::new();
        path.move_to(crate::types::Point::new(4, 12))
            .line_to(crate::types::Point::new(44, 12));
        let paint = Paint::LinearGradient(LinearGradient {
            start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
            end: mirx::types::Point::new(mirx::types::Fixed::ONE, mirx::types::Fixed::ZERO),
            stops: Cow::Owned(alloc::vec![
                GradientStop {
                    offset: mirx::types::Fixed::ZERO,
                    color: mirx::types::Color::rgb(0, 0, 0),
                },
                GradientStop {
                    offset: mirx::types::Fixed::ONE,
                    color: mirx::types::Color::rgb(255, 0, 0),
                },
            ]),
            spread: SpreadMode::Pad,
            units: GradientUnits::ObjectBoundingBox,
            transform: mirx::types::Transform::IDENTITY,
        });
        let command = DrawCommand::StrokePath {
            path: &path,
            transform: Transform::IDENTITY,
            paint: &paint,
            width: Fixed::from_int(4),
            opa: 255,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: Fixed::from_int(4),
            dash: &[],
        };
        let clip = Rect::new(0, 0, 48, 24);
        assert_eq!(renderer.submit(&DrawRequest::new(&command, clip)), Ok(()));
        assert!((100..=150).contains(&renderer.target.get_pixel(24, 12).r));
    }

    #[test]
    fn checked_radial_fill_renders_the_focal_circle() {
        let mut renderer = SwRenderer::new(Texture::owned(104, 54, ColorFormat::RGBA8888));
        let path = Path::rect(0.into(), 0.into(), 100.into(), 50.into());
        let paint = Paint::RadialGradient(RadialGradient {
            center: mirx::types::Point::new(
                mirx::types::Fixed::from_ratio(1, 2),
                mirx::types::Fixed::from_ratio(1, 2),
            ),
            radius: mirx::types::Fixed::from_ratio(1, 2),
            focal: mirx::types::Point::new(
                mirx::types::Fixed::from_ratio(1, 4),
                mirx::types::Fixed::from_ratio(1, 2),
            ),
            focal_radius: mirx::types::Fixed::ZERO,
            stops: Cow::Owned(alloc::vec![
                GradientStop {
                    offset: mirx::types::Fixed::ZERO,
                    color: mirx::types::Color::rgb(0, 0, 0),
                },
                GradientStop {
                    offset: mirx::types::Fixed::ONE,
                    color: mirx::types::Color::rgb(255, 0, 0),
                },
            ]),
            spread: SpreadMode::Pad,
            units: GradientUnits::ObjectBoundingBox,
            transform: mirx::types::Transform::IDENTITY,
        });
        let command = DrawCommand::FillPath {
            path: &path,
            transform: Transform::IDENTITY,
            paint: &paint,
            opa: 255,
            fill_rule: FillRule::EvenOdd,
        };
        let clip = Rect::new(0, 0, 104, 54);
        assert_eq!(renderer.submit(&DrawRequest::new(&command, clip)), Ok(()));
        assert!((83..=87).contains(&renderer.target.get_pixel(50, 25).r));
        assert!(renderer.target.get_pixel(98, 25).r > 240);
    }
}

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
        let phys_clip = self.viewport.rect_to_physical(*clip);
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
        let gradient = match paint {
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
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let solid_color = match paint {
            Paint::Color(color) => (*color).into(),
            _ => Color::rgba(255, 255, 255, 255),
        };
        let color_a_norm =
            Fixed::from_int(solid_color.a as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let combined_alpha = opa_norm * color_a_norm;

        let segs = &scratch.flatten_buf;
        let target_w = self.target.width as usize;
        let clip_mask = scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let target = &mut self.target;
        let acc = &mut scratch.scanline_acc;
        let crossings = &mut scratch.scanline_crossings;
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
                    if let Some(gradient) = gradient.as_ref() {
                        let c = gradient.sample(px, py);
                        let paint_alpha =
                            ((u16::from(final_alpha) * u16::from(c.a) + 127) / 255) as u8;
                        target.blend_pixel_int(px, py, &c, paint_alpha);
                    } else {
                        target.blend_pixel_int(px, py, &solid_color, final_alpha);
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
        if opa == 0 || width <= Fixed::ZERO {
            return;
        }
        let phys_tf = self.viewport.as_transform();
        let phys_width = width * self.viewport.scale();
        {
            let scratch = &mut *self.scratch;
            raster::offset_polygon_into(
                &path.cmds,
                Some(&phys_tf),
                phys_width,
                cap,
                join,
                miter_limit,
                if dash.is_empty() { None } else { Some(dash) },
                &mut scratch.stroke_outline,
                &mut scratch.flatten_buf,
                &mut scratch.subpath_scratch,
                &mut scratch.stroke_normals,
                &mut scratch.stroke_rail,
                &mut scratch.stroke_left_rail,
                &mut scratch.stroke_arc,
                &mut scratch.dash_segments,
                &mut scratch.dash_scratch,
            );
        }
        let outline_cmds = core::mem::take(&mut self.scratch.stroke_outline);
        let phys_clip = self.viewport.rect_to_physical(*clip);
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
                &mut scratch.stroke_normals,
                &mut scratch.stroke_rail,
                &mut scratch.stroke_left_rail,
                &mut scratch.stroke_arc,
                &mut scratch.dash_segments,
                &mut scratch.dash_scratch,
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
        let gradient = match paint {
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
        let opa_norm = Fixed::from_int(opa as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let solid_color = match paint {
            Paint::Color(color) => (*color).into(),
            _ => Color::rgba(255, 255, 255, 255),
        };
        let color_a_norm =
            Fixed::from_int(solid_color.a as i32).map_range((0, 255), (Fixed::ZERO, Fixed::ONE));
        let combined_alpha = opa_norm * color_a_norm;

        let segs = &scratch.flatten_buf;
        let target_w = self.target.width as usize;
        let clip_mask = scratch.clip_stack.last().map(|m| m.alpha.as_slice());
        let target = &mut self.target;
        let acc = &mut scratch.scanline_acc;
        let crossings = &mut scratch.scanline_crossings;
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
                    if let Some(gradient) = gradient.as_ref() {
                        let c = gradient.sample(px, py);
                        let paint_alpha =
                            ((u16::from(final_alpha) * u16::from(c.a) + 127) / 255) as u8;
                        target.blend_pixel_int(px, py, &c, paint_alpha);
                    } else {
                        target.blend_pixel_int(px, py, &solid_color, final_alpha);
                    }
                }
            },
        );
    }
}

#[cfg(all(test, feature = "std"))]
mod gradient_tests {
    extern crate std;
    use super::*;
    use crate::prelude::Point;
    use crate::render::canvas::Canvas;
    use crate::render::path::Path;
    use crate::render::texture::{ColorFormat, Texture};
    use mirx::scene::{GradientStop, GradientUnits, LinearGradient, SpreadMode};

    fn linear_paint_obb() -> Paint {
        Paint::LinearGradient(LinearGradient {
            start: mirx::types::Point {
                x: Fixed::ZERO.into(),
                y: Fixed::ZERO.into(),
            },
            end: mirx::types::Point {
                x: Fixed::ONE.into(),
                y: Fixed::ONE.into(),
            },
            stops: std::borrow::Cow::Owned(std::vec![
                GradientStop {
                    offset: Fixed::ZERO.into(),
                    color: Color::rgb(50, 120, 255).into()
                },
                GradientStop {
                    offset: Fixed::ONE.into(),
                    color: Color::rgb(255, 70, 90).into()
                },
            ]),
            spread: SpreadMode::Pad,
            units: GradientUnits::ObjectBoundingBox,
            transform: Transform::IDENTITY.into(),
        })
    }

    #[test]
    fn linear_object_bounding_box_maps_to_path_bbox() {
        let mut buf = std::vec![0u8; 64 * 64 * 4];
        let tex = Texture::new(&mut buf, 64, 64, ColorFormat::RGBA8888);
        let mut backend = SwRenderer::new(tex);
        let clip = Rect::new(0, 0, 64, 64);

        let mut path = Path::new();
        path.move_to(Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(10),
        });
        path.line_to(Point {
            x: Fixed::from_int(50),
            y: Fixed::from_int(10),
        });
        path.line_to(Point {
            x: Fixed::from_int(50),
            y: Fixed::from_int(50),
        });
        path.line_to(Point {
            x: Fixed::from_int(10),
            y: Fixed::from_int(50),
        });
        path.close();

        let paint = linear_paint_obb();
        backend.fill_path(&path, &clip, &paint, 255, FillRule::EvenOdd);

        let tl = backend.target.get_pixel(12, 12);
        let br = backend.target.get_pixel(48, 48);
        std::eprintln!("tl={:?} br={:?}", tl, br);
        assert!(
            tl.b > tl.r,
            "top-left (near gradient start) should be blue-ish, got {:?}",
            tl
        );
        assert!(
            br.r > br.b,
            "bottom-right (near gradient end) should be red-ish, got {:?}",
            br
        );
    }
}
