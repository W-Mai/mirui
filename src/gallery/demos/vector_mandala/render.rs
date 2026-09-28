use super::scene::{decode_emblem, rebuild_frame};
use super::state::VectorMandala;
use crate::prelude::*;
use crate::render::font::Font;
use crate::render::renderer::Renderer;
use crate::render::scene::resolver::SliceResolver;
use crate::render::texture::Texture;
use crate::ui::view::{View, ViewCtx};
use crate::ui::widgets::assets::IMG_THUMBS_UP;

fn vector_mandala_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(state) = world.get::<VectorMandala>(entity) else {
        return;
    };
    let now_ms = world
        .resource::<MonoClock>()
        .map(|c| c.now_ms())
        .unwrap_or(0);
    let elapsed_ms = now_ms.wrapping_sub(state.start_ms) as i32;
    let spin_deg = Fixed::from_int((elapsed_ms * 360 / 16000) % 360);

    let cx = rect.x + rect.w / Fixed::from_int(2);
    let cy = rect.y + rect.h / Fixed::from_int(2);

    let mut emblem = state.emblem.borrow_mut();
    if emblem.is_none() {
        *emblem = Some(decode_emblem());
    }
    let emblem = emblem
        .as_ref()
        .expect("the vector mandala emblem is initialized");
    let mut scene = state.frame.borrow_mut();
    rebuild_frame(&mut scene, cx, cy, state.petals, spin_deg, emblem);

    let fonts: [(&str, &Font); 0] = [];
    let textures: [(&str, &Texture); 1] = [("thumbs_up", &IMG_THUMBS_UP)];
    let resolver = SliceResolver::new(&fonts, &textures);
    ctx.replay(renderer, &scene.ops, &resolver);
}

pub fn vector_mandala_view() -> View {
    View::new("VectorMandala", 60, vector_mandala_render).with_filter::<VectorMandala>()
}

#[mirui_macros::system(order = ANIMATION)]
pub fn vector_mandala_anim_system(world: &mut World) {
    world.for_each_stable::<VectorMandala>(|world, e| {
        world.invalidate(e);
    });
}
