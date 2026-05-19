use alloc::vec::Vec;

use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::types::{Fixed, Point, Rect};
use crate::widget::view::{View, ViewCtx};

pub struct Text(pub Vec<u8>);

fn text_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(text) = world.get::<Text>(entity) else {
        return;
    };
    let color = ctx.style.text_color.resolve_in(ctx.theme(world), ctx.state);
    renderer.draw(
        &DrawCommand::Label {
            pos: Point {
                x: rect.x + Fixed::from_int(2),
                y: rect.y + Fixed::from_int(2),
            },
            transform: ctx.transform,
            text: &text.0,
            color,
            opa: 255,
        },
        ctx.clip,
    );
}

pub fn view() -> View {
    View::new("Text", 80, text_render)
}
