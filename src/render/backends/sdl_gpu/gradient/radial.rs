use core::hash::{Hash, Hasher};
use mirx::scene::{GradientStop, Paint, SpreadMode};
use sdl2_sys::{SDL_Color, SDL_FPoint, SDL_Vertex};

use super::{Axis, BATCH_VERTICES, MAX_STOPS, Vertex, cut, valid_stops};
use crate::render::backends::sdl_gpu::tessellation::TessellationCache;
use crate::render::backends::sdl_gpu::{SdlGpuRenderer, draw_geometry, sdl_pixel_rect};
use crate::render::command::DrawCommand;
use crate::render::paint::RadialPaint;
use crate::render::{DrawRequest, RenderError};
use crate::types::{Color, Fixed, Fixed64, Viewport};

const MAX_PALETTE_SIZE: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Key {
    pub size: u16,
    count: u8,
    radius: Fixed,
    stops: [GradientStop; MAX_STOPS],
}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.size.hash(state);
        self.count.hash(state);
        self.radius.hash(state);
        for stop in self.stops {
            stop.offset.hash(state);
            [stop.color.r, stop.color.g, stop.color.b, stop.color.a].hash(state);
        }
    }
}

impl Key {
    pub(crate) fn bytes(&self) -> usize {
        usize::from(self.size) * usize::from(self.size) * 4
    }
}

pub(crate) struct Radial<'a> {
    pub key: Key,
    space: [f64; 6],
    sampler: RadialPaint<'a>,
    last: Color,
}

pub(crate) fn plan<'a>(request: &DrawRequest<'_, 'a>, viewport: Viewport) -> Option<Radial<'a>> {
    let DrawCommand::FillPath {
        path,
        transform,
        paint: Paint::RadialGradient(gradient),
        ..
    } = request.command
    else {
        return None;
    };
    if gradient.spread != SpreadMode::Pad || !valid_stops(&gradient.stops) {
        return None;
    }
    // Canonical corners require a discriminant up to 2*r^4 in Q48.16.
    if gradient.radius > mirx::types::Fixed::from_int(2048) {
        return None;
    }
    let sampler = RadialPaint::new(
        gradient,
        viewport.as_transform().compose(transform),
        path.bbox()?,
    )?;
    let mut space = sampler.centered_space()?;
    let radius = gradient.radius.to_f64();
    let mut slope = 0.0_f64;
    let mut first_slope = 0.0_f64;
    for (index, pair) in gradient.stops.windows(2).enumerate() {
        let delta = pair[1].offset.to_f64() - pair[0].offset.to_f64();
        let a = pair[0].color;
        let b = pair[1].color;
        let contrast = [
            a.r.abs_diff(b.r),
            a.g.abs_diff(b.g),
            a.b.abs_diff(b.b),
            a.a.abs_diff(b.a),
        ]
        .into_iter()
        .max()?;
        let value = f64::from(contrast) / delta;
        slope = slope.max(value);
        if index == 0 {
            first_slope = value;
        }
    }
    // Reject fields whose Q48.16 distance quantization already exceeds the sampling allowance.
    let fixed_error = first_slope
        * ((2.0 / (65536.0 * radius * radius)).sqrt() + 1.0 / (65536.0 * radius * radius));
    if !fixed_error.is_finite() || fixed_error > 4.0 {
        return None;
    }
    // Bilinear interpolation of a Lipschitz radial field has error <= sqrt(2)*slope/(size-1).
    let needed = (core::f64::consts::SQRT_2 * slope / 2.0).ceil() as usize + 1;
    let size = needed.max(32).checked_next_power_of_two()?;
    if size > MAX_PALETTE_SIZE {
        return None;
    }
    space[2] -= (space[0] + space[1]) * 0.5;
    space[5] -= (space[3] + space[4]) * 0.5;
    if space.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let empty = GradientStop {
        offset: mirx::types::Fixed::ZERO,
        color: mirx::types::Color::rgba(0, 0, 0, 0),
    };
    let mut stops = [empty; MAX_STOPS];
    stops[..gradient.stops.len()].copy_from_slice(&gradient.stops);
    Some(Radial {
        key: Key {
            size: size as u16,
            count: gradient.stops.len() as u8,
            radius: gradient.radius.into(),
            stops,
        },
        space,
        sampler,
        last: gradient.stops.last()?.color.into(),
    })
}

impl Radial<'_> {
    fn coordinate(&self, index: usize) -> f32 {
        index as f32 * 2.0 / (f32::from(self.key.size) - 1.0) - 1.0
    }

    pub(crate) fn texel(&self, x: usize, y: usize) -> Color {
        let size = usize::from(self.key.size);
        if x == 0 || y == 0 || x == size - 1 || y == size - 1 {
            return self.last;
        }
        self.sampler
            .sample_centered(self.coordinate(x), self.coordinate(y))
    }

    pub(crate) fn write_pixels(&self, pixels: &mut [u8], pitch: usize) -> Result<(), RenderError> {
        let size = usize::from(self.key.size);
        let end = pitch
            .checked_mul(size - 1)
            .and_then(|offset| offset.checked_add(size * 4))
            .ok_or(RenderError::InvalidTexture)?;
        if pitch < size * 4 || pixels.len() < end {
            return Err(RenderError::InvalidTexture);
        }

        // Floating-point reflection can quantize differently. Reuse only equal magnitudes.
        let radius = self.key.radius.to_f32();
        let mut reflected = [false; MAX_PALETTE_SIZE / 2];
        for (index, equal) in reflected[..size / 2].iter_mut().enumerate().skip(1) {
            let near = Fixed64::from_f32(self.coordinate(index) * radius);
            let far = Fixed64::from_f32(self.coordinate(size - 1 - index) * radius);
            *equal = near.abs() == far.abs();
        }
        let mut write = |x: usize, y: usize, color: Color| {
            let offset = y * pitch + x * 4;
            pixels[offset..offset + 4].copy_from_slice(&[color.r, color.g, color.b, color.a]);
        };
        for index in 0..size {
            write(index, 0, self.last);
            write(index, size - 1, self.last);
            write(0, index, self.last);
            write(size - 1, index, self.last);
        }
        for y in 1..size / 2 {
            let (previous, remaining) = pixels.split_at_mut(y * pitch);
            let (row, remaining) = remaining.split_at_mut(pitch);
            self.write_row(row, y, &reflected, previous, pitch);
            let opposite_y = size - 1 - y;
            let offset = (opposite_y - y - 1) * pitch;
            let opposite = &mut remaining[offset..offset + size * 4];
            if reflected[y] {
                opposite.copy_from_slice(&row[..size * 4]);
            } else {
                self.write_row(opposite, opposite_y, &reflected, &[], pitch);
            }
        }
        Ok(())
    }

    fn write_row(
        &self,
        row: &mut [u8],
        y: usize,
        reflected: &[bool],
        previous: &[u8],
        pitch: usize,
    ) {
        let size = usize::from(self.key.size);
        let (near, far) = row[..size * 4].split_at_mut(size * 2);
        for (x, (near, far)) in near
            .chunks_exact_mut(4)
            .zip(far.rchunks_exact_mut(4))
            .enumerate()
            .skip(1)
        {
            if x < y && !previous.is_empty() {
                // Coordinate exchange preserves the centered squared distance exactly.
                let offset = x * pitch + y * 4;
                near.copy_from_slice(&previous[offset..offset + 4]);
            } else {
                let color = self.texel(x, y);
                near.copy_from_slice(&[color.r, color.g, color.b, color.a]);
            }
            if reflected[x] {
                far.copy_from_slice(near);
            } else {
                let color = self.texel(size - 1 - x, y);
                far.copy_from_slice(&[color.r, color.g, color.b, color.a]);
            }
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
        let size = f64::from(self.key.size);
        let mut emit = |polygon: &[Vertex], outside: bool| -> Result<(), RenderError> {
            for i in 1..polygon.len().saturating_sub(1) {
                if batch.len() + 3 > BATCH_VERTICES {
                    draw(batch)?;
                    batch.clear();
                }
                for p in [polygon[0], polygon[i], polygon[i + 1]] {
                    let uv = |value: f64| ((0.5 + (size - 1.0) * value) / size) as f32;
                    batch.push(SDL_Vertex {
                        position: SDL_FPoint {
                            x: p.x as f32,
                            y: p.y as f32,
                        },
                        color: SDL_Color {
                            r: 255,
                            g: 255,
                            b: 255,
                            a: opacity,
                        },
                        tex_coord: SDL_FPoint {
                            x: uv(if outside { 0.0 } else { p.t.clamp(0.0, 1.0) }),
                            y: uv(if outside { 0.0 } else { p.v.clamp(0.0, 1.0) }),
                        },
                    });
                }
            }
            Ok(())
        };
        for triangle in tess.indices.chunks_exact(3) {
            let mut first = [Vertex::default(); 8];
            let mut second = [Vertex::default(); 8];
            for (out, &index) in first.iter_mut().zip(triangle) {
                let p = tess.verts[index as usize].position;
                *out = Vertex {
                    x: p.x as f64,
                    y: p.y as f64,
                    t: self.space[0] * p.x as f64 + self.space[1] * p.y as f64 + self.space[2],
                    v: self.space[3] * p.x as f64 + self.space[4] * p.y as f64 + self.space[5],
                };
            }
            let mut count = 3;
            for (axis, edge, above) in [
                (Axis::U, 0.0, true),
                (Axis::U, 1.0, false),
                (Axis::V, 0.0, true),
                (Axis::V, 1.0, false),
            ] {
                if count == 0 {
                    break;
                }
                let outside = cut(&first[..count], &mut second, edge, !above, axis);
                emit(&second[..outside], true)?;
                count = cut(&first[..count], &mut second, edge, above, axis);
                core::mem::swap(&mut first, &mut second);
            }
            emit(&first[..count], false)?;
        }
        if !batch.is_empty() {
            draw(batch)?;
        }
        Ok(())
    }
}

impl<S: AsRef<[u8]> + AsMut<[u8]>> SdlGpuRenderer<'_, S> {
    pub(crate) fn radial_gradient(
        &mut self,
        request: &DrawRequest<'_, '_>,
    ) -> Result<(), RenderError> {
        let plan = plan(request, self.viewport).ok_or(RenderError::InvalidGeometry)?;
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
        self.tessellator.fill(
            path.commands(),
            Some(&self.viewport.as_transform().compose(transform)),
            &Color::rgb(255, 255, 255),
            255,
            *fill_rule,
        )?;
        let clip = self.physical_draw_clip(request.clip);
        let Some(clip) = sdl_pixel_rect(&clip, &clip) else {
            return Ok(());
        };
        let device = self.device;
        let tess = &mut *self.tessellator;
        self.texture_cache.with_radial(&plan, |texture| {
            let mut canvas = device.canvas()?;
            canvas.set_clip_rect(clip);
            let result = plan.emit(tess, *opa, |vertices| {
                draw_geometry(&mut canvas, Some(texture), vertices, None)
            });
            device.finish_draw(&mut canvas);
            result
        })?
    }
}

#[cfg(test)]
#[path = "../../../../../tests/unit/render/backends/sdl_gpu/gradient/radial.rs"]
pub(crate) mod tests;
