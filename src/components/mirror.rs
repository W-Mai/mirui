//! Mirror reflection of another entity. Reads the source's current-
//! frame texture and paints it flipped vertically into the mirror's
//! own rect, with optional fade.
//!
//! The source must be rendered before the mirror in walker order
//! (i.e. earlier in the children array). When the source's texture
//! isn't available yet (first frame, or source out of order), the
//! mirror skips silently.

use crate::draw::texture::{ColorFormat, Texture};
use crate::ecs::{Entity, World};
use crate::types::{Point, Rect};
use crate::widget::offscreen::{WidgetTextureAccess, WidgetTextureRef};
use crate::widget::view::{View, ViewCtx};

pub struct MirrorOf {
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

fn mirror_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(mir) = world.get::<MirrorOf>(entity) else {
        return;
    };
    let Some(snap) = world.texture_of(mir.source) else {
        return;
    };
    let src = snap.borrow();
    flip_into(&src, ctx.transform, rect, ctx.clip, mir.fade, renderer);
}

/// Vertically flip `src` and blit it into `rect` on `renderer`. Fade
/// linearly attenuates alpha along the y-axis: row 0 is fully opaque,
/// row h-1 picks up `fade/255` of additional transparency.
fn flip_into(
    src: &Texture,
    transform: crate::types::Transform,
    rect: &Rect,
    clip: &Rect,
    fade: u8,
    renderer: &mut dyn Renderer,
) {
    use crate::draw::command::DrawCommand;
    let Ok(_) = ensure_rgba(src) else {
        return;
    };

    let w = src.width;
    let h = src.height;
    let mut tmp = Texture::owned(w, h, ColorFormat::RGBA8888);
    let src_buf = src.buf.as_slice();
    let src_stride = src.stride;
    if let crate::draw::texture::TexBuf::Owned(ref mut dst_buf) = tmp.buf {
        let dst_stride = w as usize * 4;
        for y in 0..h as usize {
            let src_row = (h as usize - 1 - y) * src_stride;
            let dst_row = y * dst_stride;
            // Per-row fade: closer to the bottom of the *output* gets
            // more transparency, which matches the visual intuition of
            // a reflection fading away from its source.
            let row_fade = (fade as i32 * y as i32 / h as i32) as u8;
            let alpha_scale = 255i32 - row_fade as i32;
            for x in 0..w as usize {
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

    let cmd = DrawCommand::Blit {
        pos: Point::new(rect.x, rect.y),
        size: Point::new(rect.w, rect.h),
        transform,
        quad: None,
        texture: &tmp,
    };
    renderer.draw(&cmd, clip);
}

fn ensure_rgba(tex: &Texture) -> Result<(), ()> {
    match tex.format {
        ColorFormat::RGBA8888 => Ok(()),
        _ => Err(()),
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
    // Add OffscreenRender up front so the first frame's
    // texture_of(source) hits cache; maintain_widget_texture_refs
    // does the same on its first tick, but that tick runs after the
    // initial render.
    use crate::widget::offscreen::{OffscreenAlphaMode, OffscreenAutoAdded};
    if world
        .get::<crate::widget::OffscreenRender>(source)
        .is_none()
    {
        world.insert(source, crate::widget::OffscreenRender::default());
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
    View::new("MirrorOf", 60, mirror_render).with_attach(mirror_attach)
}

use crate::draw::renderer::Renderer;
