use super::{SwRenderer, write_pixels};
use crate::render::{DrawCommand, PreparedDraw, RenderError, texture::Region};
use crate::types::{Rect, Transform3D, TransformClass};

impl SwRenderer<'_> {
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
mod tests {
    use super::*;
    use crate::render::{
        CompositeMode, DrawRequest, Renderer,
        texture::{AlphaMode, ColorFormat, Texture},
    };
    use crate::types::{Color, Fixed, PhysicalRect, Point, Transform, Viewport};

    #[test]
    fn prepared_regions_match_full_sources_without_allocating() {
        for format in [
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
            ColorFormat::RGB888,
        ] {
            let mut source = Texture::owned(7, 5, format).unwrap();
            {
                let mut pixels = source.pixels_mut().unwrap();
                for y in 0..5 {
                    for x in 0..7 {
                        pixels.set_pixel(
                            x,
                            y,
                            &Color::rgba(30 + x as u8 * 25, 30 + y as u8 * 35, 113, 177),
                        );
                    }
                }
            }
            for scale in [Fixed::ONE, Fixed::from_ratio(3, 2), Fixed::from_int(2)] {
                for (pos, size) in [
                    (Point::new(1, 2), Point::new(7, 5)),
                    (Point::new(-1, -2), Point::new(14, 10)),
                    (
                        Point::new(Fixed::HALF, Fixed::from_ratio(-1, 4)),
                        Point::new(Fixed::from_ratio(31, 4), 9),
                    ),
                    (Point::new(1, 1), Point::new(4, 3)),
                ] {
                    for (opa, radius, composite) in [
                        (255, Fixed::ZERO, CompositeMode::SourceOver),
                        (117, Fixed::from_int(2), CompositeMode::SourceOver),
                        (177, Fixed::ZERO, CompositeMode::Add),
                    ] {
                        let command = DrawCommand::Blit {
                            texture: &source,
                            pos,
                            size,
                            transform: Transform::IDENTITY,
                            quad: None,
                            opa,
                            radius,
                            composite,
                        };
                        let request = DrawRequest::new(&command, Rect::new(2, 2, 7, 6))
                            .with_physical_clip(PhysicalRect::new(3, 4, 8, 7).unwrap());
                        let renderer = || {
                            let mut renderer = SwRenderer::new(
                                Texture::owned(24, 24, ColorFormat::RGBA8888).unwrap(),
                            )
                            .with_alpha_mode(AlphaMode::Blend);
                            renderer.viewport = Viewport::new(24, 24, scale);
                            renderer
                        };
                        let mut full = renderer();
                        full.submit_raw(&request).unwrap();
                        let mut regional = renderer();
                        assert_eq!(
                            crate::test_support::tracked_allocations(|| regional
                                .draw_request(&request)
                                .unwrap()),
                            0
                        );
                        assert_eq!(
                            regional.target.read_pixels().unwrap().bytes(),
                            full.target.read_pixels().unwrap().bytes(),
                            "format={format:?}, scale={scale:?}, pos={pos:?}, size={size:?}, opa={opa}, radius={radius:?}"
                        );
                    }
                }
            }
        }
    }
}
