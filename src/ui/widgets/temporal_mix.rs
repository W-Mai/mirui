//! Temporal IIR mix of `source`. Each frame's output is
//! `α * prev_output + (1 - α) * source_curr`, where α = `mix / 255`.
//! The recursion gives a real exponential decay across many frames:
//! a step in `source` colour fades over several frames at α=0.75
//! (mix=190) instead of resolving in one.
//!
//! `mix=0` shows only the current source; `mix=255` freezes the
//! initial frame.
//!
//! Source must render before this widget in walker order. The
//! widget's own offscreen buffer holds last frame's output and is
//! reused as `prev_output` next frame.

use crate::ecs::{Entity, World};
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::render::sw::mix::mix_inplace;
use crate::render::texture::{ColorFormat, Texture};
use crate::types::{Point, Rect};
use crate::ui::offscreen::{
    OffscreenAlphaMode, OffscreenAutoAdded, WidgetTextureAccess, WidgetTextureRef,
};
use crate::ui::view::{View, ViewCtx};

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
    if let crate::render::texture::TexBuf::Owned(ref mut dst) = tmp.buf {
        dst.copy_from_slice(curr.buf.as_slice());
    }
    drop(curr);

    // Feedback: mix `prev_output` (this widget's own previous frame
    // texture, generation - 1) into `tmp`. Without this the effect
    // resolves a colour change in a single frame because both
    // operands are source inputs from adjacent frames; with it the
    // recursion `out_n = α*out_{n-1} + (1-α)*src_n` decays over many.
    if let Some(prev_out) = world.prev_texture_of(entity) {
        let p = prev_out.borrow();
        if p.width == tmp.width && p.height == tmp.height && p.format == tmp.format {
            mix_inplace(&mut tmp, &p, tm.mix);
        }
    }

    renderer.draw(
        &DrawCommand::Blit {
            pos: Point::new(rect.x, rect.y),
            size: Point::new(rect.w, rect.h),
            transform: ctx.transform,
            quad: ctx.quad,
            texture: &tmp,
            opa: 255,
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
    if world.get::<crate::ui::OffscreenRender>(source).is_none() {
        world.insert(source, crate::ui::OffscreenRender::default());
        world.insert(source, OffscreenAutoAdded);
    }
    if world.get::<OffscreenAlphaMode>(source).is_none() {
        world.insert(source, OffscreenAlphaMode::clear_transparent());
    }
    // The widget's own offscreen buffer stores the previous output
    // for the IIR feedback. clear_transparent keeps initialisation
    // from picking up framebuffer pixels under the widget rect.
    if world.get::<crate::ui::OffscreenRender>(entity).is_none() {
        world.insert(entity, crate::ui::OffscreenRender::default());
        world.insert(entity, OffscreenAutoAdded);
    }
    if world.get::<OffscreenAlphaMode>(entity).is_none() {
        world.insert(entity, OffscreenAlphaMode::clear_transparent());
    }
}

pub fn view() -> View {
    View::new("TemporalMix", 60, temporal_mix_render)
        .with_filter::<TemporalMix>()
        .with_attach(temporal_mix_attach)
}
