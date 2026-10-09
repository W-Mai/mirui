//! Mirror reflection of another entity. Reads the source's completed texture and paints it flipped vertically into the mirror's own rect, with optional fade.
//!
//! Referenced sources are prepared before composition. An unavailable source produces no reflection.

use crate::ecs::{Entity, World};
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Fixed, Point, Rect};
use crate::ui::offscreen::{WidgetTexture, WidgetTextureRef};
use crate::ui::outputs::Outputs;
use crate::ui::view::{PaintCtx, View};

pub struct MirrorOf {
    pub source: Entity,
    pub fade: u8,
}

impl crate::ecs::Component for MirrorOf {}

impl MirrorOf {
    pub fn new(source: Entity) -> Self {
        Self { source, fade: 0 }
    }

    pub fn with_fade(mut self, fade: u8) -> Self {
        self.fade = fade;
        self
    }
}

fn mirror_render(world: &World, entity: Entity, paint: &mut PaintCtx<'_>) {
    let Some(mir) = world.get::<MirrorOf>(entity) else {
        return;
    };
    let Some(source) = world
        .resource::<Outputs<WidgetTexture>>()
        .and_then(|outputs| outputs.get(mir.source))
    else {
        return;
    };
    flip_into(source.texture(), paint.rect(), mir.fade, paint);
}

/// Vertically flip `src` and blit it into `rect`. Fade
/// linearly attenuates alpha along the y-axis: row 0 is fully opaque,
/// row h-1 picks up `fade/255` of additional transparency.
fn flip_into(src: &Texture, rect: Rect, fade: u8, paint: &mut PaintCtx<'_>) {
    use crate::render::command::{CompositeMode, DrawCommand};

    let w = src.width;
    let h = src.height;
    let mut tmp = Texture::owned(w, h, ColorFormat::RGBA8888);
    if let crate::render::texture::TexBuf::Owned(ref mut dst_buf) = tmp.buf {
        let dst_stride = w as usize * 4;
        match src.format {
            ColorFormat::RGBA8888 => flip_rgba8888(src, dst_buf, dst_stride, fade),
            ColorFormat::RGB565 => flip_rgb565(src, dst_buf, dst_stride, fade, false),
            ColorFormat::RGB565Swapped => flip_rgb565(src, dst_buf, dst_stride, fade, true),
            ColorFormat::RGB888 | ColorFormat::BGRA8888 => return,
        }
    }

    let cmd = DrawCommand::Blit {
        pos: Point::new(rect.x, rect.y),
        size: Point::new(rect.w, rect.h),
        transform: paint.transform(),
        quad: None,
        texture: &tmp,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    paint.draw(&cmd);
}

fn flip_rgba8888(src: &Texture, dst_buf: &mut [u8], dst_stride: usize, fade: u8) {
    let w = src.width as usize;
    let h = src.height as usize;
    let src_buf = src.buf.as_slice();
    let src_stride = src.stride;
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

fn flip_rgb565(src: &Texture, dst_buf: &mut [u8], dst_stride: usize, fade: u8, swapped: bool) {
    let w = src.width as usize;
    let h = src.height as usize;
    let src_buf = src.buf.as_slice();
    let src_stride = src.stride;
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

fn mirror_attach(world: &mut World, entity: Entity) {
    let Some(mir) = world.get::<MirrorOf>(entity) else {
        return;
    };
    let source = mir.source;
    if world.get::<WidgetTextureRef>(entity).is_none() {
        world.insert(entity, WidgetTextureRef(source));
    }
    // Request the source before the initial render prepares its output.
    use crate::ui::offscreen::{OffscreenAlphaMode, OffscreenAutoAdded};
    if world.get::<crate::ui::OffscreenRender>(source).is_none() {
        world.insert(source, crate::ui::OffscreenRender::default());
        world.insert(source, OffscreenAutoAdded);
    }
    // Skip framebuffer pre-seed: pre-seeded pixels outside the
    // source's drawn area would surface in the flipped blit (e.g.
    // the mirror's own previous reflection from below).
    if world.get::<OffscreenAlphaMode>(source).is_none() {
        world.insert(source, OffscreenAlphaMode::clear_transparent());
    }
}

pub fn view() -> View {
    View::new("MirrorOf", 60, mirror_render)
        .with_filter::<MirrorOf>()
        .with_attach(mirror_attach)
}
