use core::task::{Context, Poll};

use super::texture::{Region, TextureRegion};
use super::{DrawCommand, DrawRequest, RenderError, RenderRoute};
use crate::types::{Fixed, PhysicalRect, Rect, Transform3D, TransformClass};

/// A routed command with source storage retained for submission.
///
/// Command descriptions are copied; their resources remain borrowed. Preparation never borrows the destination renderer or allocates storage for the command.
pub struct PreparedDraw<'data> {
    command: DrawCommand<'data>,
    clip: Rect,
    projective: Transform3D,
    physical_clip: Option<PhysicalRect>,
    route: RenderRoute,
    scale: Fixed,
    source: Option<TextureRegion<'data>>,
}

impl<'data> PreparedDraw<'data> {
    /// Retain a conservative source footprint for nearest or bilinear device sampling.
    ///
    /// Integer-aligned draws include filtering neighbors; fractional placement and arbitrary transforms retain the complete source. The backend must preserve original texture coordinates and track uploaded coverage. This does not prepare mipmaps or wider filter kernels.
    pub fn poll_upload(
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        scale: Fixed,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self, RenderError>> {
        let mut draw = Self::native(request, route, scale)?;
        if let DrawCommand::Blit { texture, .. } = *request.command {
            let area = upload_region(&request, scale)?;
            draw.source = Some(core::task::ready!(texture.poll_pixels_in(area, cx))?);
        }
        Poll::Ready(Ok(draw))
    }
    /// Retain the complete source for backends that require full-image uploads or CPU fallback.
    ///
    /// The returned request keeps its original clip and geometry. Full-source preparation uses the same source cache and admission budget as regional preparation.
    pub fn poll_complete(
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        scale: Fixed,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self, RenderError>> {
        let mut draw = Self::native(request, route, scale)?;
        if let DrawCommand::Blit { texture, .. } = *request.command {
            let area = Region::new(0, 0, u32::from(texture.width), u32::from(texture.height))
                .map_err(|_| RenderError::InvalidTexture)?;
            draw.source = Some(core::task::ready!(texture.poll_pixels_in(area, cx))?);
        }
        Poll::Ready(Ok(draw))
    }
    pub fn request(&self) -> DrawRequest<'_, 'data> {
        DrawRequest {
            command: &self.command,
            clip: self.clip,
            projective: self.projective,
            physical_clip: self.physical_clip,
        }
    }

    pub fn route(&self) -> RenderRoute {
        self.route
    }

    pub fn output_scale(&self) -> Fixed {
        self.scale
    }

    pub fn texture_region(&self) -> Option<&TextureRegion<'data>> {
        self.source.as_ref()
    }

    /// Retain CPU source pixels without changing their original geometry or sampling phase.
    pub fn poll_cpu(
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        output_scale: Fixed,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self, RenderError>> {
        let mut draw = Self::native(request, route, output_scale)?;
        if let DrawCommand::Blit { texture, .. } = *request.command {
            let area = source_region(&request, output_scale)?;
            draw.source = Some(core::task::ready!(texture.poll_pixels_in(area, cx))?);
        }
        Poll::Ready(Ok(draw))
    }

    /// Retain native source identity without forcing CPU readback.
    ///
    /// The backend must verify device compatibility and target feedback before submitting.
    pub fn native(
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        output_scale: Fixed,
    ) -> Result<Self, RenderError> {
        if output_scale <= Fixed::ZERO {
            return Err(RenderError::InvalidGeometry);
        }
        request.validate()?;
        Ok(Self {
            command: *request.command,
            clip: request.clip,
            projective: request.projective,
            physical_clip: request.physical_clip,
            route,
            scale: output_scale,
            source: None,
        })
    }
}

fn upload_region(request: &DrawRequest<'_, '_>, scale: Fixed) -> Result<Region, RenderError> {
    let DrawCommand::Blit {
        texture,
        pos,
        size,
        transform,
        quad,
        opa,
        ..
    } = *request.command
    else {
        return Err(RenderError::InvalidGeometry);
    };
    let full = Region::new(0, 0, u32::from(texture.width), u32::from(texture.height))
        .map_err(|_| RenderError::InvalidTexture)?;
    if opa == 0 {
        return Region::new(0, 0, 0, 0).map_err(|_| RenderError::InvalidGeometry);
    }
    if !request.projective.is_identity()
        || quad.is_some()
        || !matches!(
            transform.classify(),
            TransformClass::Identity | TransformClass::Translate
        )
    {
        return Ok(full);
    }
    let pos = transform.apply_point(pos);
    let area = Rect::new(pos.x * scale, pos.y * scale, size.x * scale, size.y * scale);
    // Fractional device placement differs among rasterizers. Retain the complete
    // source for that case rather than reuse software edge-sampling arithmetic.
    if !area.is_aligned() {
        return Ok(full);
    }
    let clip = Rect::new(
        request.clip.x * scale,
        request.clip.y * scale,
        request.clip.w * scale,
        request.clip.h * scale,
    );
    let clip = super::renderer::constrain_physical_clip(clip, request.physical_clip);
    let Some(visible) = area.intersect(&clip) else {
        return Region::new(0, 0, 0, 0).map_err(|_| RenderError::InvalidGeometry);
    };
    let (x0, y0, x1, y1) = visible.pixel_bounds();
    let range = |start: i32, end: i32, origin: Fixed, extent: Fixed, count: u16| {
        let count = i64::from(count);
        let size = i64::from(extent.to_int());
        // Include the complete pixel intervals and a texel on either side for
        // filtering. Integer math also covers device pixel-center sampling.
        let first = (i64::from(start - origin.to_int()) * count / size - 1).clamp(0, count);
        let last =
            ((i64::from(end - origin.to_int()) * count + size - 1) / size + 1).clamp(0, count);
        (first as u32, last as u32)
    };
    let (left, right) = range(x0, x1, area.x, area.w, texture.width);
    let (top, bottom) = range(y0, y1, area.y, area.h, texture.height);
    Region::new(left, top, right - left, bottom - top).map_err(|_| RenderError::InvalidGeometry)
}

fn source_region(request: &DrawRequest<'_, '_>, scale: Fixed) -> Result<Region, RenderError> {
    let DrawCommand::Blit {
        texture,
        pos,
        size,
        transform,
        quad,
        opa,
        ..
    } = *request.command
    else {
        return Err(RenderError::InvalidGeometry);
    };
    let full = Region::new(0, 0, u32::from(texture.width), u32::from(texture.height))
        .map_err(|_| RenderError::InvalidTexture)?;
    if opa == 0 {
        return Region::new(0, 0, 0, 0).map_err(|_| RenderError::InvalidGeometry);
    }
    if !request.projective.is_identity()
        || quad.is_some()
        || !matches!(
            transform.classify(),
            TransformClass::Identity | TransformClass::Translate
        )
    {
        return Ok(full);
    }
    let pos = transform.apply_point(pos);
    let area = Rect::new(pos.x * scale, pos.y * scale, size.x * scale, size.y * scale);
    let clip = Rect::new(
        request.clip.x * scale,
        request.clip.y * scale,
        request.clip.w * scale,
        request.clip.h * scale,
    );
    let clip = super::renderer::constrain_physical_clip(clip, request.physical_clip);
    let Some(visible) = area.intersect(&clip) else {
        return Region::new(0, 0, 0, 0).map_err(|_| RenderError::InvalidGeometry);
    };
    if area.w <= Fixed::ZERO || area.h <= Fixed::ZERO {
        return Err(RenderError::InvalidGeometry);
    }
    let (x0, y0, x1, y1) = visible.pixel_bounds();
    let range = |first: i32, end: i32, origin: Fixed, extent: Fixed, samples: u16| {
        if area.is_aligned() {
            let step = super::sampling::dda_step(samples, extent.to_int());
            let sample = |pixel: i32| ((pixel - origin.to_int()) as u32).wrapping_mul(step) >> 16;
            (sample(first), sample(end - 1) + 1)
        } else {
            let sample = |pixel| {
                super::sampling::SampleAxis::new(pixel, origin, extent, i32::from(samples))
                    .next(i32::from(samples)) as u32
            };
            (sample(first), sample(end - 1) + 1)
        }
    };
    let (left, right) = range(x0, x1, area.x, area.w, texture.width);
    let (top, bottom) = range(y0, y1, area.y, area.h, texture.height);
    Region::new(left, top, right - left, bottom - top).map_err(|_| RenderError::InvalidGeometry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{
        CompositeMode, Renderer, SwRenderer,
        texture::{ColorFormat, Texture},
    };
    use crate::test_support::tracked_allocations;
    use crate::types::{Point, Transform};

    fn command<'a>(texture: &'a Texture<'a>, pos: Point, size: Point) -> DrawCommand<'a> {
        DrawCommand::Blit {
            texture,
            pos,
            size,
            transform: Transform::IDENTITY,
            quad: None,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        }
    }

    #[test]
    fn upload_footprint_covers_pixel_centers_and_filter_neighbors() {
        let bytes = [0u8; 400 * 4];
        let source = Texture::from_ref(&bytes, 400, 1, ColorFormat::RGBA8888).unwrap();
        let cmd = command(&source, Point::ZERO, Point::new(4, 1));
        let request = DrawRequest::new(&cmd, Rect::new(1, 0, 1, 1));
        let mut cx = Context::from_waker(core::task::Waker::noop());
        let Poll::Ready(Ok(draw)) =
            PreparedDraw::poll_upload(request, RenderRoute::Native, Fixed::ONE, &mut cx)
        else {
            panic!("resident upload");
        };
        let area = draw.texture_region().unwrap().region();
        assert_eq!(area, Region::new(99, 0, 102, 1).unwrap());
        // A center sample at 150 must not use the software left-edge footprint.
        assert!(draw.texture_region().unwrap().get_pixel(150, 0).is_some());
    }

    #[test]
    fn preparation_retains_resources_not_temporary_command_descriptions() {
        let bytes = [255; 8 * 8 * 4];
        let texture = Texture::from_ref(&bytes, 8, 8, ColorFormat::RGBA8888).unwrap();
        let mut cx = Context::from_waker(core::task::Waker::noop());
        assert_eq!(
            tracked_allocations(|| {
                let draw = {
                    let cmd = command(&texture, Point::new(2, 3), Point::new(8, 8));
                    let Poll::Ready(Ok(draw)) = PreparedDraw::poll_cpu(
                        DrawRequest::new(&cmd, Rect::new(4, 6, 3, 2)),
                        RenderRoute::Native,
                        Fixed::ONE,
                        &mut cx,
                    ) else {
                        panic!("static source must be ready");
                    };
                    draw
                };
                let source = draw.texture_region().unwrap();
                assert_eq!(source.region(), Region::new(2, 3, 3, 2).unwrap());
                assert_eq!(
                    source.pixels().unwrap().bytes().as_ptr(),
                    bytes[104..].as_ptr()
                );
                assert_eq!(draw.request().clip, Rect::new(4, 6, 3, 2));
                assert_eq!(draw.route(), RenderRoute::Native);
            }),
            0
        );
    }

    #[test]
    fn prepared_region_contains_every_sample_of_the_existing_rasterizer() {
        let bytes: [u8; 7 * 5 * 4] = core::array::from_fn(|i| match i % 4 {
            0 => (i / 4 % 7 + 1) as u8,
            1 => (i / 4 / 7 + 1) as u8,
            _ => 255,
        });
        let texture = Texture::from_ref(&bytes, 7, 5, ColorFormat::RGBA8888).unwrap();
        let mut cx = Context::from_waker(core::task::Waker::noop());
        for scale in [Fixed::ONE, Fixed::from_ratio(3, 2), Fixed::from_int(2)] {
            for offset in [-193, 0, 128, 256] {
                for extent in [128, 1024, 1792, 3584] {
                    for clip in [Rect::new(0, 0, 16, 16), Rect::new(2, 1, 3, 4)] {
                        let mut cmd = command(
                            &texture,
                            Point::new(Fixed::from_ratio(offset, 256), Fixed::HALF),
                            Point::new(Fixed::from_ratio(extent, 256), 5),
                        );
                        for opacity in [117, 255] {
                            if let DrawCommand::Blit { opa, .. } = &mut cmd {
                                *opa = opacity;
                            }
                            let request = DrawRequest::new(&cmd, clip)
                                .with_physical_clip(PhysicalRect::new(1, 1, 13, 13).unwrap());
                            let Poll::Ready(Ok(draw)) = PreparedDraw::poll_cpu(
                                request,
                                RenderRoute::Native,
                                scale,
                                &mut cx,
                            ) else {
                                panic!("static source must be ready");
                            };
                            let target = Texture::owned(24, 24, ColorFormat::RGBA8888).unwrap();
                            let mut renderer = SwRenderer::new(target)
                                .with_alpha_mode(crate::render::texture::AlphaMode::Blend);
                            renderer.viewport = crate::types::Viewport::new(24, 24, scale);
                            renderer.draw_request(&request).unwrap();
                            let pixels = renderer.target.read_pixels().unwrap();
                            for y in 0..24 {
                                for x in 0..24 {
                                    let color = pixels.get_pixel(x, y);
                                    if color.a != 0 {
                                        assert!(
                                            draw.texture_region()
                                                .unwrap()
                                                .get_pixel(
                                                    u32::from(color.r - 1),
                                                    u32::from(color.g - 1)
                                                )
                                                .is_some(),
                                            "offset={offset} extent={extent} scale={scale:?} pixel={color:?}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn transformed_requests_preserve_geometry_and_conservatively_prepare_the_source() {
        let bytes = [255; 4 * 4 * 4];
        let texture = Texture::from_ref(&bytes, 4, 4, ColorFormat::RGBA8888).unwrap();
        let mut cmd = command(&texture, Point::new(1, 2), Point::new(8, 8));
        if let DrawCommand::Blit { transform, .. } = &mut cmd {
            *transform = Transform::scale(Fixed::from_int(2), Fixed::ONE);
        }
        let mut cx = Context::from_waker(core::task::Waker::noop());
        let Poll::Ready(Ok(draw)) = PreparedDraw::poll_cpu(
            DrawRequest::new(&cmd, Rect::new(2, 3, 1, 1)),
            RenderRoute::Native,
            Fixed::ONE,
            &mut cx,
        ) else {
            panic!("static source must be ready");
        };
        assert_eq!(
            draw.texture_region().unwrap().region(),
            Region::new(0, 0, 4, 4).unwrap()
        );
        assert_eq!(draw.request().command.transform(), cmd.transform());
    }
}
