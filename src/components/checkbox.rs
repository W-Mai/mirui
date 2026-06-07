use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::gesture::GestureEvent;
use crate::types::Rect;
use crate::widget::dirty::Dirty;
use crate::widget::theme::{ColorToken, ThemedColor};
use crate::widget::view::{View, ViewCtx};

#[derive(Clone, Debug)]
pub enum CheckboxEvent {
    Toggled { now: bool },
}

pub struct CheckboxHandler {
    pub on_event: fn(&mut World, Entity, &CheckboxEvent) -> bool,
}

pub struct Checkbox {
    pub checked: bool,
    pub checked_color: ThemedColor,
    pub unchecked_color: ThemedColor,
}

impl Default for Checkbox {
    fn default() -> Self {
        Self {
            checked: false,
            checked_color: ThemedColor::Token(ColorToken::Primary),
            unchecked_color: ThemedColor::Token(ColorToken::SurfaceVariant),
        }
    }
}

impl Checkbox {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_checked_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.checked_color = color.into();
        self
    }

    pub fn with_unchecked_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.unchecked_color = color.into();
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
        cb.checked_color.resolve_in(theme, ctx.state)
    } else {
        cb.unchecked_color.resolve_in(theme, ctx.state)
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
        let now = if let Some(cb) = world.get_mut::<Checkbox>(entity) {
            cb.toggle();
            cb.checked
        } else {
            return false;
        };
        emit_checkbox_event(world, entity, &CheckboxEvent::Toggled { now });
        world.insert(entity, Dirty);
        return true;
    }
    false
}

fn emit_checkbox_event(world: &mut World, entity: Entity, event: &CheckboxEvent) {
    let cb = world.get::<CheckboxHandler>(entity).map(|h| h.on_event);
    if let Some(f) = cb {
        f(world, entity, event);
    }
}

fn checkbox_attach(world: &mut World, entity: Entity) {
    let _ = world;
    let _ = entity;
}

pub fn view() -> View {
    View::new("Checkbox", 40, checkbox_render)
        .with_filter::<Checkbox>()
        .with_attach(checkbox_attach)
        .with_internal_gesture(checkbox_handler)
}
