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
        Self::validate(&request, scale)?;
        let source = if let DrawCommand::Blit { texture, .. } = *request.command {
            let area = upload_region(&request, scale)?;
            Some(core::task::ready!(texture.poll_pixels_in(area, cx))?)
        } else {
            None
        };
        Poll::Ready(Ok(Self::ready(request, route, scale, source)))
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
        Self::validate(&request, scale)?;
        let source = if let DrawCommand::Blit { texture, .. } = *request.command {
            let area = Region::new(0, 0, u32::from(texture.width), u32::from(texture.height))
                .map_err(|_| RenderError::InvalidTexture)?;
            Some(core::task::ready!(texture.poll_pixels_in(area, cx))?)
        } else {
            None
        };
        Poll::Ready(Ok(Self::ready(request, route, scale, source)))
    }
    pub fn request(&self) -> DrawRequest<'_, 'data> {
        DrawRequest {
            command: &self.command,
            clip: self.clip,
            projective: self.projective,
            physical_clip: self.physical_clip,
        }
    }

    pub(super) fn command(&self) -> &DrawCommand<'data> {
        &self.command
    }

    pub(super) fn clip(&self) -> &Rect {
        &self.clip
    }

    pub(super) fn projective(&self) -> &Transform3D {
        &self.projective
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
    ///
    /// Direct CPU storage remains a complete borrowed view. Sources requiring preparation retain a sampling footprint instead; no full-image readback or decoding is implied.
    pub fn poll_cpu(
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        output_scale: Fixed,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self, RenderError>> {
        Self::validate(&request, output_scale)?;
        let source = if let DrawCommand::Blit { texture, .. } = *request.command {
            Some(match texture.direct_region()? {
                Some(pixels) => pixels,
                None => {
                    let area = source_region(&request, output_scale)?;
                    core::task::ready!(texture.poll_pixels_in(area, cx))?
                }
            })
        } else {
            None
        };
        Poll::Ready(Ok(Self::ready(request, route, output_scale, source)))
    }

    /// Retain native source identity without forcing CPU readback.
    ///
    /// The backend must verify device compatibility and target feedback before submitting.
    pub fn native(
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        output_scale: Fixed,
    ) -> Result<Self, RenderError> {
        Self::validate(&request, output_scale)?;
        Ok(Self::ready(request, route, output_scale, None))
    }

    fn validate(request: &DrawRequest<'_, '_>, output_scale: Fixed) -> Result<(), RenderError> {
        if output_scale <= Fixed::ZERO {
            return Err(RenderError::InvalidGeometry);
        }
        request.validate()
    }

    fn ready(
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        scale: Fixed,
        source: Option<TextureRegion<'data>>,
    ) -> Self {
        Self {
            command: *request.command,
            clip: request.clip,
            projective: request.projective,
            physical_clip: request.physical_clip,
            route,
            scale,
            source,
        }
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
                super::sampling::SampleAxis::at(pixel, origin, extent, i32::from(samples)) as u32
            };
            (sample(first), sample(end - 1) + 1)
        }
    };
    let (left, right) = range(x0, x1, area.x, area.w, texture.width);
    let (top, bottom) = range(y0, y1, area.y, area.h, texture.height);
    Region::new(left, top, right - left, bottom - top).map_err(|_| RenderError::InvalidGeometry)
}

#[cfg(test)]
#[path = "../../tests/unit/render/prepared.rs"]
mod tests;
