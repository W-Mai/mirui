use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
use crate::types::{Color, Rect};
use crate::widget::dirty::Dirty;
use crate::widget::view::{View, ViewCtx};

#[derive(Default)]
pub struct Checkbox {
    pub checked: bool,
    pub checked_color: Option<Color>,
    pub unchecked_color: Option<Color>,
}

impl Checkbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_checked_color(mut self, color: Color) -> Self {
        self.checked_color = Some(color);
        self
    }

    pub fn with_unchecked_color(mut self, color: Color) -> Self {
        self.unchecked_color = Some(color);
        self
    }

    pub fn toggle(&mut self) {
        self.checked = !self.checked;
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
    let theme = ctx.theme(world);
    let color = if cb.checked {
        cb.checked_color.unwrap_or(theme.primary)
    } else {
        cb.unchecked_color.unwrap_or(theme.surface_variant)
    };
    renderer.draw(
        &DrawCommand::Fill {
            area: *rect,
            transform: ctx.transform,
            quad: ctx.quad,
            color,
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
    View::new("Checkbox", 40, checkbox_render).with_attach(checkbox_attach)
}
