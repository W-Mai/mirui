use crate::components::button::Button;
use crate::components::checkbox::Checkbox;
use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::types::{Fixed, Rect};

use super::Style;
use super::view::ViewCtx;

pub(crate) fn style_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(style) = world.get::<Style>(entity) else {
        return;
    };

    // Temporary: Button/Checkbox own their bg via cascade here.
    // Will move into per-widget render fns when those land.
    let bg = if let Some(btn) = world.get::<Button>(entity) {
        Some(btn.current_color())
    } else if let Some(cb) = world.get::<Checkbox>(entity) {
        Some(cb.current_color())
    } else {
        style.bg_color
    };

    if let Some(color) = bg {
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
