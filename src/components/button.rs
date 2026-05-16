use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::widget_input::button_handler;
use crate::types::{Color, Rect};
use crate::widget::view::{View, ViewCtx};

pub struct Button {
    pub pressed: bool,
    pub normal_color: Color,
    pub pressed_color: Color,
}

impl Button {
    pub fn new(normal: Color, pressed: Color) -> Self {
        Self {
            pressed: false,
            normal_color: normal,
            pressed_color: pressed,
        }
    }

    pub fn current_color(&self) -> Color {
        if self.pressed {
            self.pressed_color
        } else {
            self.normal_color
        }
    }
}

fn button_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(btn) = world.get::<Button>(entity) else {
        return;
    };
    renderer.draw(
        &DrawCommand::Fill {
            area: *rect,
            transform: ctx.transform,
            quad: ctx.quad,
            color: btn.current_color(),
            radius: ctx.style.border_radius,
            opa: 255,
        },
        ctx.clip,
    );
    ctx.bg_handled = true;
}

fn button_attach(world: &mut World, entity: Entity) {
    if world.get::<Button>(entity).is_none() {
        return;
    }
    if world.get::<GestureHandler>(entity).is_some() {
        return;
    }
    world.insert(
        entity,
        GestureHandler {
            on_gesture: button_handler,
        },
    );
}

pub fn view() -> View {
    View {
        name: "Button",
        priority: 40,
        render: button_render,
        auto_attach: Some(button_attach),
    }
}
