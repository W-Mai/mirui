//! Mirror reflection of another entity. Reads the source's completed texture and paints it flipped vertically into the mirror's own rect, with optional fade.
//!
//! Referenced sources are prepared before composition. An unavailable source produces no reflection.

use crate::ecs::{Entity, World};
use crate::render::PlaneLayout;
use crate::render::texture::{AlphaMode, ColorFormat, Texture, TexturePixels};
use crate::types::{Fixed, Point, Rect};
use crate::ui::offscreen::WidgetTexture;
use crate::ui::outputs::Outputs;
use crate::ui::view::{PaintCtx, PaintMeasurement, SourceGeometry, View};

#[crate::component]
pub struct MirrorOf {
    #[depends_on(WidgetTexture)]
    pub source: Entity,
    pub fade: u8,
}

impl MirrorOf {
    pub fn new(source: Entity) -> Self {
        Self { source, fade: 0 }
    }

    pub fn with_fade(mut self, fade: u8) -> Self {
        self.fade = fade;
        self
    }
}

#[crate::view(component = MirrorOf, name = "MirrorOf", priority = 60)]
fn mirror_render(mirror: component!(), outputs: res!(Outputs<WidgetTexture>), paint: paint!()) {
    let Some(source) = outputs.get(mirror.source) else {
        return;
    };
    flip_into(source.texture(), paint.rect(), mirror.fade, paint);
}

fn mirror_measurement(
    world: &World,
    entity: Entity,
    rect: Rect,
    source_geometry: &dyn Fn(Entity) -> Option<SourceGeometry>,
) -> Option<PaintMeasurement> {
    let mirror = world.get::<MirrorOf>(entity)?;
    let source = source_geometry(mirror.source)?.plane;
    let workspace = match source.format() {
        ColorFormat::RGBA8888 | ColorFormat::RGB565 | ColorFormat::RGB565Swapped => {
            PlaneLayout::packed(source.width(), source.height(), ColorFormat::RGBA8888).ok()?
        }
        ColorFormat::RGB888 | ColorFormat::BGRA8888 => return None,
    };
    Some(PaintMeasurement {
        coverage: crate::ui::dirty::PaintCoverage::solid(rect),
        workspace: Some(workspace),
    })
}

/// Vertically flip `src` into `rect`, attenuating alpha progressively down the reflection.
fn flip_into(src: &Texture, rect: Rect, fade: u8, paint: &mut PaintCtx<'_>) {
    use crate::render::command::{CompositeMode, DrawCommand};

    if matches!(src.format, ColorFormat::RGB888 | ColorFormat::BGRA8888) {
        return;
    }
    let Ok(layout) = PlaneLayout::packed(src.width, src.height, ColorFormat::RGBA8888) else {
        return;
    };
    paint.with_workspace(layout, |texture, paint| {
        texture.alpha_mode = AlphaMode::Blend;
        texture.transient = true;
        let Some(source) = paint.read_pixels(src) else {
            return;
        };
        let mut destination = match texture.pixels_mut() {
            Ok(pixels) => pixels,
            Err(error) => {
                paint.fail(error);
                return;
            }
        };
        let dst_stride = destination.stride();
        let dst_buf = destination.bytes_mut();
        match source.format() {
            ColorFormat::RGBA8888 => flip_rgba8888(&source, dst_buf, dst_stride, fade),
            ColorFormat::RGB565 => flip_rgb565(&source, dst_buf, dst_stride, fade, false),
            ColorFormat::RGB565Swapped => flip_rgb565(&source, dst_buf, dst_stride, fade, true),
            ColorFormat::RGB888 | ColorFormat::BGRA8888 => return,
        }

        paint.draw(&DrawCommand::Blit {
            pos: Point::new(rect.x, rect.y),
            size: Point::new(rect.w, rect.h),
            transform: paint.transform(),
            quad: None,
            texture,
            opa: 255,
            radius: Fixed::ZERO,
            composite: CompositeMode::SourceOver,
        });
    });
}

fn flip_rgba8888(src: &TexturePixels<'_>, dst_buf: &mut [u8], dst_stride: usize, fade: u8) {
    let w = src.width() as usize;
    let h = src.height() as usize;
    let src_buf = src.bytes();
    let src_stride = src.stride();
    for y in 0..h {
        let src_row = (h - 1 - y) * src_stride;
        let dst_row = y * dst_stride;
        let row_fade = (fade as i32 * y as i32 / h as i32) as u8;
        let alpha_scale = 255i32 - row_fade as i32;
        for x in 0..w {
            let si = src_row + x * 4;
            let di = dst_row + x * 4;
            dst_buf[di] = src_buf[si];
            dst_buf[di + 1] = src_buf[si + 1];
            dst_buf[di + 2] = src_buf[si + 2];
            let a = src_buf[si + 3] as i32;
            dst_buf[di + 3] = ((a * alpha_scale) / 255) as u8;
        }
    }
}

fn flip_rgb565(
    src: &TexturePixels<'_>,
    dst_buf: &mut [u8],
    dst_stride: usize,
    fade: u8,
    swapped: bool,
) {
    let w = src.width() as usize;
    let h = src.height() as usize;
    let src_buf = src.bytes();
    let src_stride = src.stride();
    for y in 0..h {
        let src_row = (h - 1 - y) * src_stride;
        let dst_row = y * dst_stride;
        let row_fade = (fade as i32 * y as i32 / h as i32) as u8;
        let alpha_scale = (255i32 - row_fade as i32) as u8;
        for x in 0..w {
            let si = src_row + x * 2;
            let di = dst_row + x * 4;
            let (lo, hi) = if swapped {
                (src_buf[si + 1], src_buf[si])
            } else {
                (src_buf[si], src_buf[si + 1])
            };
            let pixel = ((hi as u16) << 8) | (lo as u16);
            let r5 = ((pixel >> 11) & 0x1F) as u8;
            let g6 = ((pixel >> 5) & 0x3F) as u8;
            let b5 = (pixel & 0x1F) as u8;
            dst_buf[di] = (r5 << 3) | (r5 >> 2);
            dst_buf[di + 1] = (g6 << 2) | (g6 >> 4);
            dst_buf[di + 2] = (b5 << 3) | (b5 >> 2);
            dst_buf[di + 3] = alpha_scale;
        }
    }
}

pub fn view() -> View {
    mirror_render::view().with_measurement(mirror_measurement)
}
