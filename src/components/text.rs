use alloc::vec::Vec;

use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::types::{Color, Fixed, Point, Rect};
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
    let color = ctx.style.text_color.unwrap_or(Color::rgb(255, 255, 255));
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
    View {
        name: "Text",
        priority: 80,
        render: text_render,
        auto_attach: None,
        systems: &[],
    }
}
