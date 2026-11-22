//! Native paint sampling over the existing path geometry.
pub(super) mod radial;
use mirx::scene::{GradientStop, Paint, SpreadMode};
use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_Vertex};

use super::tessellation::TessellationCache;
use super::{SdlGpuRenderer, draw_geometry, sdl_pixel_rect};
use crate::render::command::DrawCommand;
use crate::render::paint::{LinearPaint, sample_stops};
use crate::render::{DrawRequest, RenderError};
use crate::types::{Color, Fixed64, Viewport};

const MAX_STOPS: usize = 16;
pub(super) const BATCH_VERTICES: usize = 768;

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
    v: f64,
}

#[derive(Clone, Copy)]
enum Axis {
    U,
    V,
}
impl Axis {
    fn get(self, vertex: Vertex) -> f64 {
        match self {
            Self::U => vertex.t,
            Self::V => vertex.v,
        }
    }
}

fn cut(input: &[Vertex], output: &mut [Vertex; 8], edge: f64, above: bool, axis: Axis) -> usize {
    let mut n = 0;
    let mut previous = input[input.len() - 1];
    let inside = |v: Vertex| {
        if above {
            axis.get(v) >= edge
        } else {
            axis.get(v) <= edge
        }
    };
    for &current in input {
        if inside(current) != inside(previous) {
            let amount = (edge - axis.get(previous)) / (axis.get(current) - axis.get(previous));
            output[n] = Vertex {
                x: previous.x + (current.x - previous.x) * amount,
                y: previous.y + (current.y - previous.y) * amount,
                t: previous.t + (current.t - previous.t) * amount,
                v: previous.v + (current.v - previous.v) * amount,
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
                    v: 0.0,
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
                        Axis::U,
                    );
                    core::mem::swap(&mut first, &mut second);
                }
                if count > 0 && band < self.stops.len() {
                    count = cut(
                        &first[..count],
                        &mut second,
                        self.stops[band].offset.to_f32() as f64,
                        false,
                        Axis::U,
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
#[path = "../../../../tests/unit/render/backends/sdl_gpu/gradient.rs"]
mod tests;
