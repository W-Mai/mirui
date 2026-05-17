use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
use crate::types::{Color, Rect};
use crate::widget::dirty::Dirty;
use crate::widget::view::{View, ViewCtx};

pub struct Checkbox {
    pub checked: bool,
    pub checked_color: Color,
    pub unchecked_color: Color,
}

impl Checkbox {
    pub fn new(checked_color: Color, unchecked_color: Color) -> Self {
        Self {
            checked: false,
            checked_color,
            unchecked_color,
        }
    }

    pub fn toggle(&mut self) {
        self.checked = !self.checked;
    }

    pub fn current_color(&self) -> Color {
        if self.checked {
            self.checked_color
        } else {
            self.unchecked_color
        }
    }
}

fn checkbox_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(cb) = world.get::<Checkbox>(entity) else {
        return;
    };
    renderer.draw(
        &DrawCommand::Fill {
            area: *rect,
            transform: ctx.transform,
            quad: ctx.quad,
            color: cb.current_color(),
            radius: ctx.style.border_radius,
            opa: 255,
        },
        ctx.clip,
    );
    ctx.bg_handled = true;
}

pub(crate) fn checkbox_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    if let GestureEvent::Tap { .. } = event {
        if let Some(cb) = world.get_mut::<Checkbox>(entity) {
            cb.toggle();
        }
        world.insert(entity, Dirty);
        return true;
    }
    false
}

fn checkbox_attach(world: &mut World, entity: Entity) {
    if world.get::<Checkbox>(entity).is_none() {
        return;
    }
    if world.get::<GestureHandler>(entity).is_some() {
        return;
    }
    world.insert(
        entity,
        GestureHandler {
            on_gesture: checkbox_handler,
        },
    );
}

pub fn view() -> View {
    View {
        name: "Checkbox",
        priority: 40,
        render: checkbox_render,
        auto_attach: Some(checkbox_attach),
        systems: &[],
    }
}
