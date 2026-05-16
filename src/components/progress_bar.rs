use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::widget_input::progress_bar_handler;
use crate::types::{Color, Fixed, Rect};
use crate::widget::view::{View, ViewCtx};

pub struct ProgressBar {
    pub value: f32, // 0.0 ~ 1.0
    pub track_color: Color,
    pub fill_color: Color,
}

impl ProgressBar {
    pub fn new(fill: Color, track: Color) -> Self {
        Self {
            value: 0.0,
            track_color: track,
            fill_color: fill,
        }
    }
}

fn progress_bar_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(pb) = world.get::<ProgressBar>(entity) else {
        return;
    };
    renderer.draw(
        &DrawCommand::Fill {
            area: *rect,
            transform: ctx.transform,
            quad: ctx.quad,
            color: pb.track_color,
            radius: ctx.style.border_radius,
            opa: 255,
        },
        ctx.clip,
    );
    let fill_w = Fixed::from_f32(rect.w.to_f32() * pb.value.clamp(0.0, 1.0));
    if fill_w > Fixed::ZERO {
        renderer.draw(
            &DrawCommand::Fill {
                area: Rect {
                    x: rect.x,
                    y: rect.y,
                    w: fill_w,
                    h: rect.h,
                },
                transform: ctx.transform,
                quad: ctx.quad,
                color: pb.fill_color,
                radius: ctx.style.border_radius,
                opa: 255,
            },
            ctx.clip,
        );
    }
}

fn progress_bar_attach(world: &mut World, entity: Entity) {
    if world.get::<ProgressBar>(entity).is_none() {
        return;
    }
    if world.get::<GestureHandler>(entity).is_some() {
        return;
    }
    world.insert(
        entity,
        GestureHandler {
            on_gesture: progress_bar_handler,
        },
    );
}

pub fn view() -> View {
    View {
        name: "ProgressBar",
        priority: 60,
        render: progress_bar_render,
        auto_attach: Some(progress_bar_attach),
    }
}
