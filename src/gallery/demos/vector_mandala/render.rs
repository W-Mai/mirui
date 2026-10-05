use super::scene::{decode_emblem, rebuild_frame};
use super::state::VectorMandala;
use crate::prelude::*;
use crate::render::font::Font;
use crate::render::scene::resolver::SliceResolver;
use crate::render::texture::Texture;
use crate::ui::widgets::assets::IMG_THUMBS_UP;

#[crate::view(component = VectorMandala, name = "VectorMandala", priority = 60)]
fn vector_mandala_render(state: component!(), clock: res!(optional MonoClock), paint: paint!()) {
    let now_ms = clock.map(|c| c.now_ms()).unwrap_or(0);
    let elapsed_ms = now_ms.wrapping_sub(state.start_ms) as i32;
    let spin_deg = Fixed::from_int((elapsed_ms * 360 / 16000) % 360);

    let rect = paint.rect();
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
    paint.replay(&scene.ops, &resolver);
}

pub use vector_mandala_render::view as vector_mandala_view;

#[mirui_macros::system(order = Animation)]
pub fn vector_mandala_anim_system(states: query!(mut VectorMandala)) {
    for _state in states {}
}
