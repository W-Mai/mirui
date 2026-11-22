use super::{SwRenderer, write_pixels};
use crate::render::{DrawCommand, PreparedDraw, RenderError, texture::Region};
use crate::types::{Rect, Transform3D, TransformClass};

impl SwRenderer<'_> {
    pub(super) fn submit_routed_primitive(
        &mut self,
        request: &crate::render::DrawRequest<'_, '_>,
    ) -> Result<(), RenderError> {
        self.flush()?;
        let saved = self.physical_clip;
        self.physical_clip = request.physical_clip;
        self.draw(request.command, &request.clip);
        let result = self.flush();
        self.physical_clip = saved;
        result
    }

    pub(super) fn submit_prepared(&mut self, draw: &PreparedDraw<'_>) -> Result<(), RenderError> {
        if draw.output_scale() != self.viewport.scale() {
            return Err(RenderError::InvalidScope);
        }
        let request = draw.request();
        if !matches!(request.command, DrawCommand::Blit { .. }) {
            return self.submit_raw(&request);
        }
        self.flush()?;
        request.validate_projection()?;
        request.validate_write_boundary(self.viewport)?;
        let saved = self.physical_clip;
        self.physical_clip = request.physical_clip;
        let result = self.blit_prepared(draw);
        self.physical_clip = saved;
        result
    }

    // Keep texture sampling and projection branches out of primitive submission frames.
    #[inline(never)]
    fn blit_prepared(&mut self, draw: &PreparedDraw<'_>) -> Result<(), RenderError> {
        crate::trace_span!("sw.blit");
        let DrawCommand::Blit {
            texture,
            pos,
            size,
            transform,
            quad,
            radius,
            opa,
            composite,
        } = *draw.command()
        else {
            return Err(RenderError::InvalidGeometry);
        };
        let region = draw.texture_region().ok_or(RenderError::InvalidTexture)?;
        if region.region().is_empty() {
            return Ok(());
        }
        let pixels = region
            .borrowed_pixels()
            .ok_or(RenderError::InvalidTexture)?;
        if draw.projective().is_identity()
            && quad.is_none()
            && matches!(
                transform.classify(),
                TransformClass::Identity | TransformClass::Translate
            )
        {
            let source = Rect::new(
                -(region.region().x() as i32),
                -(region.region().y() as i32),
                texture.width,
                texture.height,
            );
            self.blit_pixels_inner(
                pixels,
                &source,
                transform.apply_point(pos),
                size,
                draw.clip(),
                opa,
                radius,
                composite,
            );
        } else {
            let request = draw.request();
            // Transformed sampling uses the complete source coordinate domain.
            if region.region()
                != Region::new(0, 0, u32::from(texture.width), u32::from(texture.height))
                    .map_err(|_| RenderError::InvalidTexture)?
            {
                return Err(RenderError::InvalidTexture);
            }
            let area = Rect::new(pos.x, pos.y, size.x, size.y);
            if !request.projective.is_identity() {
                self.preflight_projective(request.command, &request.clip, &request.projective)?;
                let logical = request
                    .projective
                    .compose(&Transform3D::from_affine(transform));
                let quad = logical
                    .apply_rect(area)
                    .ok_or(RenderError::InvalidGeometry)?;
                self.dispatch_blit_quad_pixels(
                    &quad,
                    pixels,
                    size,
                    &request.clip,
                    radius,
                    opa,
                    composite,
                );
            } else if let Some(quad) = quad {
                self.dispatch_blit_quad_pixels(
                    &quad,
                    pixels,
                    size,
                    &request.clip,
                    radius,
                    opa,
                    composite,
                );
            } else {
                self.route(&request)?;
                let physical = self.viewport.as_transform().compose(&transform);
                let clip = self.physical_draw_clip(request.clip);
                if let Some(mut target) = write_pixels(&mut self.target, &mut self.failure) {
                    super::transformed::blit_transformed(
                        &mut target,
                        pixels,
                        &Rect::new(0, 0, texture.width, texture.height),
                        area,
                        clip,
                        &physical,
                    );
                }
            }
        }
        self.flush()
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/render/backends/sw/prepared.rs"]
mod tests;
