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
mod tests {
    use super::*;
    use crate::render::{
        CompositeMode, DrawRequest, Renderer,
        texture::{AlphaMode, ColorFormat, Texture},
    };
    use crate::types::{Color, Fixed, PhysicalRect, Point, Transform, Viewport};

    #[cfg(feature = "std")]
    #[test]
    fn raw_and_prepared_blits_record_one_primitive_span() {
        let _serial = crate::core::perf::tests::SERIAL.lock().unwrap();
        let texture = Texture::from_ref(&[255; 64], 4, 4, ColorFormat::RGBA8888).unwrap();
        for prepared in [false, true] {
            let mut renderer =
                SwRenderer::new(Texture::owned(16, 16, ColorFormat::RGBA8888).unwrap());
            let command = DrawCommand::Blit {
                pos: Point::new(2, 3),
                size: Point::new(4, 4),
                transform: Transform::IDENTITY,
                quad: None,
                texture: &texture,
                opa: 255,
                radius: Fixed::ZERO,
                composite: CompositeMode::SourceOver,
            };
            let clip = Rect::new(0, 0, 16, 16);
            let mut capture =
                crate::core::perf::Capture::new(crate::core::perf::DEFAULT_TRACE_BUDGET).unwrap();
            if prepared {
                renderer
                    .draw_request(&DrawRequest::new(&command, clip))
                    .unwrap();
            } else {
                renderer.draw(&command, &clip);
            }
            let mut blits = 0;
            let mut registered_drops = 0;
            capture.drain_events(|info, records| {
                if records.is_empty() && info.track != 0 {
                    registered_drops += info.status.dropped;
                }
                blits += records
                    .iter()
                    .filter(|record| record.span().is_some_and(|span| span.name == "sw.blit"))
                    .count();
            });
            assert_eq!(blits, 1, "prepared={prepared}");
            assert_eq!(registered_drops, 0);
            assert_eq!(capture.status().open_scopes, 0);
        }
    }

    #[test]
    fn integrated_primitives_match_independent_preparation_in_physical_scopes() {
        use core::task::{Context, Poll, Waker};
        for format in [ColorFormat::RGBA8888, ColorFormat::RGB565Swapped] {
            for scale in [Fixed::ONE, Fixed::from_ratio(3, 2), Fixed::from_int(2)] {
                for transform in [
                    Transform::IDENTITY,
                    Transform::translate(Fixed::HALF, Fixed::HALF),
                    Transform::rotate_deg(17.into()),
                ] {
                    let area = Rect::new(3, 4, 5, 7);
                    let commands = [
                        DrawCommand::Fill {
                            area,
                            transform,
                            quad: None,
                            color: Color::rgba(73, 141, 207, 181),
                            radius: Fixed::from_ratio(3, 2),
                            opa: 197,
                        },
                        DrawCommand::Border {
                            area,
                            transform,
                            quad: None,
                            color: Color::rgb(207, 97, 53),
                            width: Fixed::ONE,
                            radius: Fixed::from_int(2),
                            opa: 225,
                        },
                    ];
                    for command in &commands {
                        let make = || {
                            let mut renderer =
                                SwRenderer::new(Texture::owned(24, 24, format).unwrap())
                                    .with_alpha_mode(AlphaMode::Blend);
                            renderer.viewport = Viewport::new(24, 24, scale);
                            renderer
                        };
                        let mut expected = make();
                        let mut actual = make();
                        let request = DrawRequest::new(command, Rect::new(0, 0, 18, 18))
                            .with_physical_clip(PhysicalRect::new(2, 1, 13, 15).unwrap());
                        let mut cx = Context::from_waker(Waker::noop());
                        let route = Renderer::route(&expected, &request).unwrap();
                        let Poll::Ready(Ok(prepared)) =
                            Renderer::prepare(&mut expected, request, route, &mut cx)
                        else {
                            panic!("primitive preparation must complete");
                        };
                        Renderer::submit(&mut expected, &prepared).unwrap();
                        actual.draw_request(&request).unwrap();
                        let independent_allocations =
                            crate::test_support::tracked_allocations(|| {
                                Renderer::submit(&mut expected, &prepared).unwrap();
                            });
                        let integrated_allocations =
                            crate::test_support::tracked_allocations(|| {
                                actual.draw_request(&request).unwrap();
                            });
                        assert_eq!(
                            integrated_allocations, independent_allocations,
                            "scale {scale:?}, transform {transform:?}"
                        );
                        assert_eq!(
                            actual.target.read_pixels().unwrap().bytes(),
                            expected.target.read_pixels().unwrap().bytes()
                        );
                        assert!(actual.physical_clip.is_none());
                    }
                }
            }
        }
    }

    #[test]
    fn independent_scope_checks_and_failed_flushes_remain_strict() {
        use core::task::{Context, Poll, Waker};
        let command = DrawCommand::Fill {
            area: Rect::new(0, 0, 8, 8),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(17, 83, 149),
            radius: Fixed::ZERO,
            opa: 255,
        };
        let mut renderer = SwRenderer::new(Texture::owned(8, 8, ColorFormat::RGBA8888).unwrap());
        let request = DrawRequest::new(&command, Rect::new(0, 0, 8, 8))
            .with_physical_clip(PhysicalRect::new(1, 1, 4, 4).unwrap());
        let mut cx = Context::from_waker(Waker::noop());
        let route = Renderer::route(&renderer, &request).unwrap();
        let Poll::Ready(Ok(prepared)) = Renderer::prepare(&mut renderer, request, route, &mut cx)
        else {
            panic!("primitive preparation must complete");
        };
        renderer.viewport = Viewport::new(8, 8, Fixed::from_int(2));
        assert_eq!(
            Renderer::submit(&mut renderer, &prepared),
            Err(RenderError::InvalidScope)
        );
        renderer.viewport = Viewport::new(8, 8, Fixed::ONE);
        let old_clip = Some(PhysicalRect::new(0, 0, 2, 2).unwrap());
        renderer.physical_clip = old_clip;
        renderer.failure = Some(RenderError::BackendFailure);
        assert_eq!(
            renderer.draw_request(&request),
            Err(RenderError::BackendFailure)
        );
        assert_eq!(renderer.physical_clip, old_clip);
        assert!(
            renderer
                .target
                .read_pixels()
                .unwrap()
                .bytes()
                .iter()
                .all(|byte| *byte == 0)
        );
    }

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
