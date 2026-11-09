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
            for x in 1..=y {
                let opposite_x = size - 1 - x;
                let opposite_y = size - 1 - y;
                let near = self.texel(x, y);
                let across_x = if reflected[x] {
                    near
                } else {
                    self.texel(opposite_x, y)
                };
                let across_y = if reflected[y] {
                    near
                } else {
                    self.texel(x, opposite_y)
                };
                let across_both = if reflected[x] {
                    across_y
                } else if reflected[y] {
                    across_x
                } else {
                    self.texel(opposite_x, opposite_y)
                };
                for (x, y, color) in [
                    (x, y, near),
                    (opposite_x, y, across_x),
                    (x, opposite_y, across_y),
                    (opposite_x, opposite_y, across_both),
                ] {
                    // Coordinate exchange preserves the centered squared distance exactly.
                    write(x, y, color);
                    write(y, x, color);
                }
            }
        }
        Ok(())
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
pub(crate) mod tests {
    use super::*;
    use crate::render::{path::Path, raster::FillRule};
    use crate::types::{Rect, Transform};
    use alloc::borrow::Cow;
    use mirx::scene::{GradientUnits, RadialGradient};

    fn paint(end: Color) -> Paint {
        let zero = mirx::types::Point::new(mirx::types::Fixed::ZERO, mirx::types::Fixed::ZERO);
        Paint::RadialGradient(RadialGradient {
            center: zero,
            focal: zero,
            radius: mirx::types::Fixed::from_int(20),
            focal_radius: mirx::types::Fixed::ZERO,
            stops: Cow::Owned(alloc::vec![
                GradientStop {
                    offset: mirx::types::Fixed::ZERO,
                    color: mirx::types::Color::rgba(255, 255, 255, 255)
                },
                GradientStop {
                    offset: mirx::types::Fixed::ONE,
                    color: end.into()
                }
            ]),
            spread: SpreadMode::Pad,
            units: GradientUnits::UserSpaceOnUse,
            transform: mirx::types::Transform::IDENTITY,
        })
    }

    pub(crate) fn with_plan<R>(end: Color, f: impl FnOnce(&Radial<'_>) -> R) -> R {
        let path = Path::rounded_rect(
            (-70).into(),
            (-70).into(),
            140.into(),
            140.into(),
            Fixed::ZERO,
        );
        let paint = paint(end);
        let command = DrawCommand::FillPath {
            path: &path,
            transform: Transform::IDENTITY,
            paint: &paint,
            opa: 255,
            fill_rule: FillRule::NonZero,
        };
        let request = DrawRequest::new(&command, Rect::new(-100, -100, 200, 200));
        f(&plan(&request, Viewport::new(200, 200, Fixed::ONE)).unwrap())
    }

    #[test]
    fn palette_generation_preserves_quantized_samples_and_stride_without_allocations() {
        let path = Path::rect(0.into(), 0.into(), 100.into(), 100.into());
        for radius in [
            mirx::types::Fixed::from_ratio(1, 2),
            mirx::types::Fixed::from_ratio(133, 256),
            mirx::types::Fixed::from_ratio(230, 256),
            mirx::types::Fixed::from_int(20),
            mirx::types::Fixed::from_int(2048),
        ] {
            let mut paint = paint(Color::rgba(17, 43, 91, 3));
            let Paint::RadialGradient(gradient) = &mut paint else {
                unreachable!()
            };
            gradient.radius = radius;
            let command = DrawCommand::FillPath {
                path: &path,
                paint: &paint,
                transform: Transform::IDENTITY,
                opa: 255,
                fill_rule: FillRule::NonZero,
            };
            let mut plan = plan(
                &DrawRequest::new(&command, Rect::new(0, 0, 100, 100)),
                Viewport::new(100, 100, Fixed::ONE),
            )
            .unwrap();
            for size in [32, 64, 256, 512, MAX_PALETTE_SIZE] {
                plan.key.size = size as u16;
                let pitch = size * 4 + 12;
                let mut pixels = alloc::vec![0xA5; pitch * size + 32];
                assert_eq!(
                    crate::test_support::tracked_allocations(|| plan
                        .write_pixels(&mut pixels, pitch)
                        .unwrap()),
                    0
                );
                for y in 0..size {
                    for x in 0..size {
                        let color = plan.texel(x, y);
                        let offset = y * pitch + x * 4;
                        assert_eq!(
                            pixels[offset..offset + 4],
                            [color.r, color.g, color.b, color.a],
                            "radius={radius:?} size={size} x={x} y={y}"
                        );
                    }
                    assert!(
                        pixels[y * pitch + size * 4..(y + 1) * pitch]
                            .iter()
                            .all(|&value| value == 0xA5)
                    );
                }
                assert!(pixels[pitch * size..].iter().all(|&value| value == 0xA5));
            }
        }
    }

    #[test]
    fn palette_generation_rejects_invalid_storage_before_writing() {
        with_plan(Color::rgba(1, 2, 3, 4), |plan| {
            let size = usize::from(plan.key.size);
            let mut pixels = alloc::vec![0xA5; size * size * 4];
            for pitch in [size * 4 - 1, size * 4 + 1, usize::MAX] {
                assert_eq!(
                    plan.write_pixels(&mut pixels, pitch),
                    Err(RenderError::InvalidTexture)
                );
                assert!(pixels.iter().all(|&value| value == 0xA5));
            }
            assert_eq!(
                plan.write_pixels(&mut pixels[..size * size * 4 - 1], size * 4),
                Err(RenderError::InvalidTexture)
            );
        });
    }

    #[test]
    fn radial_admission_keeps_focal_spread_sharp_and_quantized_fields_on_exact_fallback() {
        let path = Path::rect(0.into(), 0.into(), 100.into(), 100.into());
        let accepts = |paint: &Paint| {
            let command = DrawCommand::FillPath {
                path: &path,
                paint,
                transform: Transform::IDENTITY,
                opa: 255,
                fill_rule: FillRule::NonZero,
            };
            plan(
                &DrawRequest::new(&command, Rect::new(0, 0, 100, 100)),
                Viewport::new(100, 100, Fixed::ONE),
            )
            .is_some()
        };
        let paint = paint(Color::rgba(0, 0, 0, 0));
        assert!(accepts(&paint));
        let Paint::RadialGradient(original) = paint else {
            unreachable!()
        };
        let mut gradient = original.clone();
        gradient.focal.x = mirx::types::Fixed::ONE;
        assert!(!accepts(&Paint::RadialGradient(gradient)));
        let mut gradient = original.clone();
        gradient.spread = SpreadMode::Reflect;
        assert!(!accepts(&Paint::RadialGradient(gradient)));
        let mut gradient = original.clone();
        gradient.radius = mirx::types::Fixed::from_ratio(1, 256);
        assert!(!accepts(&Paint::RadialGradient(gradient)));
        let mut gradient = original.clone();
        gradient.radius = mirx::types::Fixed::from_int(4096);
        assert!(!accepts(&Paint::RadialGradient(gradient)));
        let mut gradient = original;
        gradient.stops.to_mut()[1].offset = mirx::types::Fixed::from_ratio(1, 256);
        assert!(!accepts(&Paint::RadialGradient(gradient)));
    }

    #[test]
    fn radial_pad_partition_preserves_area_and_uses_constant_outside_samples_without_hot_allocations()
     {
        with_plan(Color::rgba(10, 20, 30, 40), |plan| {
            let path = Path::rounded_rect(
                (-70).into(),
                (-70).into(),
                140.into(),
                140.into(),
                Fixed::ZERO,
            );
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
            let mut outside = 0;
            plan.emit(&mut tess, 173, |batch| {
                for tri in batch.chunks_exact(3) {
                    let p = [tri[0], tri[1], tri[2]]
                        .map(|v| (v.position.x as f64, v.position.y as f64));
                    area += ((p[1].0 - p[0].0) * (p[2].1 - p[0].1)
                        - (p[2].0 - p[0].0) * (p[1].1 - p[0].1))
                        .abs()
                        * 0.5;
                    for vertex in tri {
                        assert!((0.0..=1.0).contains(&vertex.tex_coord.x));
                        assert!((0.0..=1.0).contains(&vertex.tex_coord.y));
                        assert_eq!(vertex.color.a, 173);
                    }
                    if tri.iter().all(|v| {
                        v.tex_coord.x == 0.5 / f32::from(plan.key.size)
                            && v.tex_coord.y == 0.5 / f32::from(plan.key.size)
                    }) {
                        outside += 1;
                    }
                }
                Ok(())
            })
            .unwrap();
            assert!((area - 19600.0).abs() < 0.02, "{area}");
            assert!(outside > 0);
            for n in 0..usize::from(plan.key.size) {
                assert_eq!(plan.texel(0, n), plan.last);
            }
            assert_eq!(
                crate::test_support::tracked_allocations(|| plan
                    .emit(&mut tess, 173, |_| Ok(()))
                    .unwrap()),
                0
            );
        });
    }
}
