//! Native paint sampling over the existing path geometry.
use mirx::scene::{GradientStop, Paint, SpreadMode};
use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_Vertex};

use super::tessellation::TessellationCache;
use super::{SdlGpuRenderer, draw_geometry, sdl_pixel_rect};
use crate::render::command::DrawCommand;
use crate::render::paint::{LinearPaint, sample_stops};
use crate::render::{DrawRequest, RenderError};
use crate::types::{Color, Fixed64, Viewport};

const MAX_STOPS: usize = 16;
const BATCH_VERTICES: usize = 768;

pub(super) struct Linear<'a> {
    plane: [f64; 3],
    stops: &'a [GradientStop],
}

fn valid_stops(stops: &[GradientStop]) -> bool {
    stops.len() <= MAX_STOPS
        && GradientStop::sequence_is_valid(stops)
        && stops.windows(2).all(|pair| pair[0].offset < pair[1].offset)
}

pub(super) fn linear<'a>(request: &DrawRequest<'_, 'a>, viewport: Viewport) -> Option<Linear<'a>> {
    let DrawCommand::FillPath {
        path,
        transform,
        paint: Paint::LinearGradient(gradient),
        ..
    } = request.command
    else {
        return None;
    };
    if gradient.spread != SpreadMode::Pad || !valid_stops(&gradient.stops) {
        return None;
    }
    let physical = viewport.as_transform().compose(transform);
    let mut plane = LinearPaint::new(gradient, physical, path.bbox()?)?.plane();
    // CPU paint samples integer pixel coordinates; SDL fragments use pixel centers.
    plane[2] -= (plane[0] + plane[1]) * 0.5;
    plane.iter().all(|v| v.is_finite()).then_some(Linear {
        plane,
        stops: &gradient.stops,
    })
}

#[derive(Clone, Copy, Default)]
struct Vertex {
    x: f64,
    y: f64,
    t: f64,
}

fn cut(input: &[Vertex], output: &mut [Vertex; 8], edge: f64, above: bool) -> usize {
    let mut n = 0;
    let mut previous = input[input.len() - 1];
    let inside = |v: Vertex| if above { v.t >= edge } else { v.t <= edge };
    for &current in input {
        if inside(current) != inside(previous) {
            let amount = (edge - previous.t) / (current.t - previous.t);
            output[n] = Vertex {
                x: previous.x + (current.x - previous.x) * amount,
                y: previous.y + (current.y - previous.y) * amount,
                t: edge,
            };
            n += 1;
        }
        if inside(current) {
            output[n] = current;
            n += 1;
        }
        previous = current;
    }
    n
}

impl Linear<'_> {
    fn color(&self, t: f64, opacity: u8) -> SDL_Color {
        let result = sample_stops(self.stops, Fixed64::from_f32(t as f32));
        SDL_Color {
            r: result.r,
            g: result.g,
            b: result.b,
            a: ((u16::from(result.a) * u16::from(opacity)) / 255) as u8,
        }
    }

    fn emit(
        &self,
        tess: &mut TessellationCache,
        opacity: u8,
        mut draw: impl FnMut(&[SDL_Vertex]) -> Result<(), RenderError>,
    ) -> Result<(), RenderError> {
        let batch = &mut tess.gradient_batch;
        batch.clear();
        if batch.capacity() < BATCH_VERTICES {
            batch
                .try_reserve_exact(BATCH_VERTICES)
                .map_err(|_| RenderError::BackendFailure)?;
        }
        for triangle in tess.indices.chunks_exact(3) {
            let mut original = [Vertex::default(); 3];
            for (point, &index) in original.iter_mut().zip(triangle) {
                let p = tess.verts[index as usize].position;
                *point = Vertex {
                    x: p.x as f64,
                    y: p.y as f64,
                    t: self.plane[0] * p.x as f64 + self.plane[1] * p.y as f64 + self.plane[2],
                };
            }
            for band in 0..=self.stops.len() {
                let mut first = [Vertex::default(); 8];
                let mut second = [Vertex::default(); 8];
                first[..3].copy_from_slice(&original);
                let mut count = 3;
                if band > 0 {
                    count = cut(
                        &first[..count],
                        &mut second,
                        self.stops[band - 1].offset.to_f32() as f64,
                        true,
                    );
                    core::mem::swap(&mut first, &mut second);
                }
                if count > 0 && band < self.stops.len() {
                    count = cut(
                        &first[..count],
                        &mut second,
                        self.stops[band].offset.to_f32() as f64,
                        false,
                    );
                    core::mem::swap(&mut first, &mut second);
                }
                for i in 1..count.saturating_sub(1) {
                    if batch.len() + 3 > BATCH_VERTICES {
                        draw(batch)?;
                        batch.clear();
                    }
                    for point in [first[0], first[i], first[i + 1]] {
                        batch.push(SDL_Vertex {
                            position: SDL_FPoint {
                                x: point.x as f32,
                                y: point.y as f32,
                            },
                            color: self.color(point.t, opacity),
                            tex_coord: SDL_FPoint { x: 0.0, y: 0.0 },
                        });
                    }
                }
            }
        }
        if !batch.is_empty() {
            draw(batch)?;
        }
        Ok(())
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    pub(super) fn linear_gradient(
        &mut self,
        request: &DrawRequest<'_, '_>,
    ) -> Result<(), RenderError> {
        let plan = linear(request, self.viewport).ok_or(RenderError::InvalidGeometry)?;
        let DrawCommand::FillPath {
            path,
            transform,
            opa,
            fill_rule,
            ..
        } = request.command
        else {
            return Err(RenderError::InvalidGeometry);
        };
        let physical = self.viewport.as_transform().compose(transform);
        self.tessellator.fill(
            path.commands(),
            Some(&physical),
            &Color::rgb(255, 255, 255),
            255,
            *fill_rule,
        )?;
        let Some(clip) = sdl_pixel_rect(
            &self.physical_draw_clip(request.clip),
            &self.physical_draw_clip(request.clip),
        ) else {
            return Ok(());
        };
        let mut canvas = self.device.canvas()?;
        canvas.set_clip_rect(clip);
        canvas.set_blend_mode(sdl2::render::BlendMode::Blend);
        let result = plan.emit(self.tessellator, *opa, |vertices| {
            draw_geometry(&mut canvas, None, vertices, None)
        });
        self.device.finish_draw(&mut canvas);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{path::Path, raster::FillRule};
    use crate::types::{Fixed, Rect, Transform};
    use alloc::borrow::Cow;
    use mirx::scene::{GradientUnits, LinearGradient};

    fn paint() -> Paint {
        Paint::LinearGradient(LinearGradient {
            start: mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO),
            end: mirx::types::Point::new(
                mirx::types::Fixed::from_int(100),
                mirx::types::Fixed::ZERO,
            ),
            stops: Cow::Owned(alloc::vec![
                GradientStop {
                    offset: mirx::types::Fixed::ZERO,
                    color: mirx::types::Color::rgb(255, 0, 0)
                },
                GradientStop {
                    offset: mirx::types::Fixed::from_ratio(1, 4),
                    color: mirx::types::Color::rgb(0, 255, 0)
                },
                GradientStop {
                    offset: mirx::types::Fixed::ONE,
                    color: mirx::types::Color::rgb(0, 0, 255)
                },
            ]),
            units: GradientUnits::UserSpaceOnUse,
            spread: SpreadMode::Pad,
            transform: mirx::types::Transform::IDENTITY,
        })
    }

    #[test]
    fn linear_bands_partition_mesh_without_crossing_color_stops_or_allocating_when_warm() {
        let paint = paint();
        let path = Path::rounded_rect((-20).into(), 0.into(), 140.into(), 100.into(), Fixed::ZERO);
        let command = DrawCommand::FillPath {
            path: &path,
            transform: Transform::IDENTITY,
            paint: &paint,
            opa: 200,
            fill_rule: FillRule::NonZero,
        };
        let viewport = Viewport::new(160, 100, Fixed::ONE);
        let plan = linear(
            &DrawRequest::new(&command, Rect::new(0, 0, 160, 100)),
            viewport,
        )
        .unwrap();
        let mut tess = TessellationCache::new();
        tess.fill(
            path.commands(),
            None,
            &Color::rgb(255, 255, 255),
            255,
            FillRule::NonZero,
        )
        .unwrap();
        let mut area = 0.0_f64;
        plan.emit(&mut tess, 200, |batch| {
            assert!(batch.len() <= BATCH_VERTICES);
            for triangle in batch.chunks_exact(3) {
                let p = [triangle[0], triangle[1], triangle[2]]
                    .map(|v| (v.position.x as f64, v.position.y as f64));
                area += ((p[1].0 - p[0].0) * (p[2].1 - p[0].1)
                    - (p[2].0 - p[0].0) * (p[1].1 - p[0].1))
                    .abs()
                    * 0.5;
                let values = triangle
                    .iter()
                    .map(|v| plan.plane[0] * v.position.x as f64 + plan.plane[2]);
                let min = values.clone().fold(f64::INFINITY, f64::min);
                let max = values.fold(f64::NEG_INFINITY, f64::max);
                for edge in [0.0, 0.25, 1.0] {
                    assert!(!(min < edge - 0.000001 && max > edge + 0.000001));
                }
                for vertex in triangle {
                    assert_eq!(vertex.color.a, 200);
                }
            }
            Ok(())
        })
        .unwrap();
        assert!((area - 14000.0).abs() < 0.02, "{area}");
        assert_eq!(
            crate::test_support::tracked_allocations(|| plan
                .emit(&mut tess, 200, |_| Ok(()))
                .unwrap()),
            0
        );
        assert_eq!(
            plan.emit(&mut tess, 200, |_| Err(RenderError::BackendFailure)),
            Err(RenderError::BackendFailure)
        );
    }

    #[test]
    fn native_linear_admission_preserves_unsupported_spreads_and_hard_edges_for_fallback() {
        let mut paint = paint();
        let path = Path::rounded_rect(0.into(), 0.into(), 100.into(), 100.into(), 0.into());
        let accepted = |paint: &Paint, transform| {
            let cmd = DrawCommand::FillPath {
                path: &path,
                transform,
                paint,
                opa: 255,
                fill_rule: FillRule::NonZero,
            };
            linear(
                &DrawRequest::new(&cmd, Rect::new(0, 0, 100, 100)),
                Viewport::new(100, 100, Fixed::ONE),
            )
            .is_some()
        };
        assert!(accepted(&paint, Transform::rotate_deg(15.into())));
        assert!(!accepted(
            &paint,
            Transform::scale(Fixed::ZERO, Fixed::ZERO)
        ));
        let Paint::LinearGradient(ref mut gradient) = paint else {
            unreachable!()
        };
        gradient.spread = SpreadMode::Repeat;
        assert!(!accepted(&paint, Transform::IDENTITY));
        let Paint::LinearGradient(ref mut gradient) = paint else {
            unreachable!()
        };
        gradient.spread = SpreadMode::Pad;
        gradient.stops.to_mut()[1].offset = mirx::types::Fixed::ZERO;
        assert!(!accepted(&paint, Transform::IDENTITY));
    }
}
