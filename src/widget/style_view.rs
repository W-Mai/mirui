use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::types::{Fixed, Rect};

use super::Style;
use super::view::{View, ViewCtx};

fn style_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(style) = world.get::<Style>(entity) else {
        return;
    };

    if !ctx.bg_handled {
        if let Some(color) = style.bg_color {
            renderer.draw(
                &DrawCommand::Fill {
                    area: *rect,
                    transform: ctx.transform,
                    quad: ctx.quad,
                    color,
                    radius: style.border_radius,
                    opa: 255,
                },
                ctx.clip,
            );
            ctx.bg_handled = true;
        }
    }

    if let Some(border_color) = style.border_color {
        if style.border_width > Fixed::ZERO {
            renderer.draw(
                &DrawCommand::Border {
                    area: *rect,
                    transform: ctx.transform,
                    quad: ctx.quad,
                    color: border_color,
                    width: style.border_width,
                    radius: style.border_radius,
                    opa: 255,
                },
                ctx.clip,
            );
        }
    }
}

pub fn view() -> View {
    View {
        name: "Style",
        priority: 50,
        render: style_render,
        auto_attach: None,
    }
}
