//! Pixel exhaust composition for the desktop and Compact frame layouts.

use super::raster::{Canvas, Flame};
use mirui::{
    render::{
        command::{CompositeMode, DrawCommand},
        renderer::RenderError,
        texture::{AlphaMode, ColorFormat, Texture, TexturePixelsMut},
    },
    types::{Color, Fixed, Fixed64, Point, Rect},
    ui::view::PaintCtx,
};
pub const TILE: usize = 256;
#[path = "pixel_flame.rs"]
mod pixel_flame;

use pixel_flame::{PixelFlame, ScreenFlame};

pub(super) fn paint_region(
    canvas: &mut Canvas<'_>,
    flames: &[Flame],
    time: Fixed,
) -> Result<(), RenderError> {
    if flames.is_empty() {
        return Ok(());
    }
    let origin = canvas.origin;
    let clip = canvas.clip;
    canvas.with_pixels(|pixels| composite_region(pixels, origin, clip, flames, time))
}

fn composite_region(
    pixels: &mut TexturePixelsMut<'_>,
    [origin_x, origin_y]: [i32; 2],
    clip: [i32; 4],
    flames: &[Flame],
    time: Fixed,
) {
    mirui::trace_span!("flight.flames");
    for &flame in flames {
        let Some(screen) = ScreenFlame::new(flame) else {
            continue;
        };
        let [left, top, right, bottom] = screen.native_clip(clip);
        if left >= right || top >= bottom {
            continue;
        }
        let flame = PixelFlame::prepare(screen, time);
        for y in top..bottom {
            let sample_y = Fixed::from_int(y) + Fixed::HALF;
            for x in left..right {
                let Some((base, emission)) =
                    flame.shade([Fixed::from_int(x) + Fixed::HALF, sample_y])
                else {
                    continue;
                };
                if base.a != 0 {
                    pixels.composite_pixel(
                        x - origin_x,
                        y - origin_y,
                        &base,
                        CompositeMode::SourceOver,
                    );
                }
                if emission.r != 0 || emission.g != 0 || emission.b != 0 {
                    pixels.composite_pixel(
                        x - origin_x,
                        y - origin_y,
                        &emission,
                        CompositeMode::Add,
                    );
                }
            }
        }
    }
}
pub struct Tile {
    pub base: Texture<'static>,
    pub emission: Texture<'static>,
    pub bounds: [Fixed; 4],
    pub active: bool,
    pub size: [u16; 2],
}
pub struct Flames {
    pub tiles: [Tile; 6],
    limit: usize,
}
impl Flames {
    pub fn new() -> Result<Self, RenderError> {
        Self::with_limit(TILE)
    }
    pub(super) fn with_limit(limit: usize) -> Result<Self, RenderError> {
        assert!(limit > 0 && limit <= TILE);
        let mut values = alloc::vec::Vec::with_capacity(6);
        for _ in 0..6 {
            let mut base = Texture::owned(limit as u16, limit as u16, ColorFormat::RGBA8888)?
                .with_cache_revision(0);
            base.alpha_mode = AlphaMode::Blend;
            let mut emission = Texture::owned(limit as u16, limit as u16, ColorFormat::RGBA8888)?
                .with_cache_revision(0);
            emission.alpha_mode = AlphaMode::Blend;
            values.push(Tile {
                base,
                emission,
                bounds: [Fixed::ZERO; 4],
                active: false,
                size: [0; 2],
            });
        }
        let tiles = match values.try_into() {
            Ok(tiles) => tiles,
            Err(_) => unreachable!("six prepared flames"),
        };
        Ok(Self { tiles, limit })
    }
    pub fn bytes(&self) -> usize {
        12 * self.limit * self.limit * 4 + core::mem::size_of::<Self>()
    }
    pub fn prepare(
        &mut self,
        flames: &[Flame],
        time: Fixed,
        display_scale: Fixed,
        viewport: [usize; 2],
    ) -> Result<(), RenderError> {
        mirui::trace_span!("flight.flames");
        for (i, tile) in self.tiles.iter_mut().enumerate() {
            tile.active = false;
            let Some(screen) = flames.get(i).copied().and_then(ScreenFlame::new) else {
                continue;
            };
            let low = screen.low.map(|value| value.max(Fixed::ZERO));
            let high: [Fixed; 2] =
                core::array::from_fn(|k| screen.high[k].min(Fixed::from_int(viewport[k] as i32)));
            if high[0] <= low[0] || high[1] <= low[1] {
                continue;
            }
            tile.bounds = [low[0], low[1], high[0] - low[0], high[1] - low[1]];
            tile.size = [
                ((tile.bounds[2] * display_scale)
                    .ceil()
                    .clamp(Fixed::ONE, Fixed::from_int(self.limit as i32)))
                .trunc_to_int() as u16,
                ((tile.bounds[3] * display_scale)
                    .ceil()
                    .clamp(Fixed::ONE, Fixed::from_int(self.limit as i32)))
                .trunc_to_int() as u16,
            ];
            let flame = PixelFlame::prepare(screen, time);
            let step = core::array::from_fn::<_, 2, _>(|k| {
                Fixed64::from_fixed(tile.bounds[2 + k]) / i64::from(tile.size[k])
            });
            let [first_x, mut y] =
                core::array::from_fn(|k| Fixed64::from_fixed(low[k]) + step[k] / 2);
            let mut base = tile.base.pixels_mut()?;
            let mut emission = tile.emission.pixels_mut()?;
            let stride = usize::from(tile.size[0]) * 4;
            let bytes = stride * usize::from(tile.size[1]);
            let base_rows = base.bytes_mut()[..bytes].chunks_exact_mut(stride);
            let emission_rows = emission.bytes_mut()[..bytes].chunks_exact_mut(stride);
            let transparent = Color::rgba(0, 0, 0, 0);
            for (base_row, emission_row) in base_rows.zip(emission_rows) {
                let mut x = first_x;
                let sample_y = y.to_fixed();
                for (base, emission) in base_row
                    .chunks_exact_mut(4)
                    .zip(emission_row.chunks_exact_mut(4))
                {
                    let (color, glow) = flame
                        .shade([x.to_fixed(), sample_y])
                        .unwrap_or((transparent, transparent));
                    base.copy_from_slice(&[color.r, color.g, color.b, color.a]);
                    emission.copy_from_slice(&[glow.r, glow.g, glow.b, glow.a]);
                    x += step[0];
                }
                y += step[1];
            }
            tile.active = true;
        }
        Ok(())
    }
    pub fn paint(&self, paint: &mut PaintCtx<'_>, origin: Point, scale: Fixed) {
        for tile in self.tiles.iter().filter(|tile| tile.active) {
            let [x, y, w, h] = tile.bounds;
            let rect = Rect::new(
                origin.x + (x) * scale,
                origin.y + (y) * scale,
                (w) * scale,
                (h) * scale,
            );
            for (texture, composite) in [
                (&tile.base, CompositeMode::SourceOver),
                (&tile.emission, CompositeMode::Add),
            ] {
                let pixels = texture.read_pixels().expect("owned flame pixels");
                let [width, height] = tile.size;
                let length = usize::from(width) * usize::from(height) * 4;
                let mut view = Texture::from_ref(
                    &pixels.bytes()[..length],
                    width,
                    height,
                    ColorFormat::RGBA8888,
                )
                .expect("bounded flame tile")
                .with_cache_revision(texture.cache_revision);
                view.alpha_mode = AlphaMode::Blend;
                paint.draw(&DrawCommand::Blit {
                    pos: Point::new(rect.x, rect.y),
                    size: Point::new(rect.w, rect.h),
                    transform: paint.transform(),
                    quad: None,
                    texture: &view,
                    opa: 255,
                    radius: Fixed::ZERO,
                    composite,
                });
            }
        }
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/flame.rs"]
mod tests;
