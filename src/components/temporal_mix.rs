//! Temporal alpha mix between `source`'s current and previous frame
//! textures. `mix=0` shows only the current frame; `mix=255` shows
//! only the previous frame; values in between blend.
//!
//! Use this to smooth out per-frame content changes (colour shifts,
//! sprite animation, scrolling text). It is **not** a screen-space
//! motion blur — translating a `WidgetTransform` doesn't write into
//! the source's offscreen buffer, so a positionally-animated source
//! produces no trail. For a positional smear, animate something
//! inside the source's subtree instead.
//!
//! The source must render before this widget in walker order so the
//! current-frame texture is available; the previous-frame texture is
//! automatic across frames as long as `WidgetTextureRef` keeps the
//! buffer alive.

use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::draw::sw::mix::mix_inplace;
use crate::draw::texture::{ColorFormat, Texture};
use crate::ecs::{Entity, World};
use crate::types::{Point, Rect};
use crate::widget::offscreen::{WidgetTextureAccess, WidgetTextureRef};
use crate::widget::view::{View, ViewCtx};

pub struct TemporalMix {
    pub source: Entity,
    pub mix: u8,
}

impl TemporalMix {
    pub fn new(source: Entity) -> Self {
        Self { source, mix: 128 }
    }

    pub fn with_mix(mut self, mix: u8) -> Self {
        self.mix = mix;
        self
    }
}

fn temporal_mix_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(tm) = world.get::<TemporalMix>(entity) else {
        return;
    };
    let Some(curr_snap) = world.texture_of(tm.source) else {
        return;
    };
    let curr = curr_snap.borrow();
    if curr.format != ColorFormat::RGBA8888 {
        return;
    }

    let mut tmp = Texture::owned(curr.width, curr.height, ColorFormat::RGBA8888);
    if let crate::draw::texture::TexBuf::Owned(ref mut dst) = tmp.buf {
        dst.copy_from_slice(curr.buf.as_slice());
    }
    drop(curr);

    if let Some(prev_snap) = world.prev_texture_of(tm.source) {
        let prev = prev_snap.borrow();
        mix_inplace(&mut tmp, &prev, tm.mix);
    }

    renderer.draw(
        &DrawCommand::Blit {
            pos: Point::new(rect.x, rect.y),
            size: Point::new(rect.w, rect.h),
            transform: ctx.transform,
            quad: ctx.quad,
            texture: &tmp,
        },
        ctx.clip,
    );
}

fn temporal_mix_attach(world: &mut World, entity: Entity) {
    let Some(tm) = world.get::<TemporalMix>(entity) else {
        return;
    };
    let source = tm.source;
    if world.get::<WidgetTextureRef>(entity).is_none() {
        world.insert(entity, WidgetTextureRef(source));
    }
    // First-frame + buffer-cleanliness fix: see mirror_attach.
    use crate::widget::offscreen::{OffscreenAlphaMode, OffscreenAutoAdded};
    if world
        .get::<crate::widget::OffscreenRender>(source)
        .is_none()
    {
        world.insert(source, crate::widget::OffscreenRender::default());
        world.insert(source, OffscreenAutoAdded);
    }
    if world.get::<OffscreenAlphaMode>(source).is_none() {
        world.insert(source, OffscreenAlphaMode::clear_transparent());
    }
}

pub fn view() -> View {
    View::new("TemporalMix", 60, temporal_mix_render).with_attach(temporal_mix_attach)
}
