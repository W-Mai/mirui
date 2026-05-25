//! Background blur: read the framebuffer's current pixels under
//! this widget's rect (i.e. whatever has already been drawn behind
//! this point in walker order), IIR-blur, paint back. No source
//! entity — the source *is* the framebuffer.
//!
//! The widget must come after the content it's meant to blur in
//! children-array order. If the renderer doesn't expose
//! `read_target_region` (`offscreen_format() == None`), the widget
//! skips silently.

use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::draw::sw::blur::{alpha_for_radius, iir_blur_inplace};
use crate::draw::texture::Texture;
use crate::ecs::{Entity, World};
use crate::types::{Point, Rect};
use crate::widget::view::{View, ViewCtx};

pub struct BackgroundBlur {
    pub radius: u8,
}

impl BackgroundBlur {
    pub fn new(radius: u8) -> Self {
        Self { radius }
    }
}

fn background_blur_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(bg) = world.get::<BackgroundBlur>(entity) else {
        return;
    };
    if bg.radius == 0 {
        return;
    }
    let Some(format) = renderer.offscreen_format() else {
        return;
    };

    let w_px = rect.w.to_int().max(1) as u16;
    let h_px = rect.h.to_int().max(1) as u16;
    let mut tmp = Texture::owned(w_px, h_px, format);
    renderer.read_target_region(rect, &mut tmp);
    iir_blur_inplace(&mut tmp, alpha_for_radius(bg.radius));

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

pub fn view() -> View {
    View::new("BackgroundBlur", 60, background_blur_render)
}
