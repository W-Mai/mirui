use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
use crate::types::{Color, Rect};
use crate::widget::dirty::Dirty;
use crate::widget::view::{View, ViewCtx};

#[derive(Default)]
pub struct Button {
    pub pressed: bool,
    pub normal_color: Option<Color>,
    pub pressed_color: Option<Color>,
}

impl Button {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_normal_color(mut self, color: Color) -> Self {
        self.normal_color = Some(color);
        self
    }

    pub fn with_pressed_color(mut self, color: Color) -> Self {
        self.pressed_color = Some(color);
        self
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
    let theme = ctx.theme(world);
    let color = if btn.pressed {
        btn.pressed_color.unwrap_or(theme.primary)
    } else {
        btn.normal_color.unwrap_or(theme.surface_variant)
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

/// Press feedback: highlight while gesture is in flight, release on
/// Tap / DragEnd. DragStart is needed because Tap is press+release in
/// one event — without it we'd never see the held state.
fn button_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    match event {
        GestureEvent::DragStart { .. } => {
            if let Some(btn) = world.get_mut::<Button>(entity) {
                btn.pressed = true;
            }
            world.insert(entity, Dirty);
            false
        }
        GestureEvent::Tap { .. } | GestureEvent::DragEnd { .. } => {
            if let Some(btn) = world.get_mut::<Button>(entity) {
                btn.pressed = false;
            }
            world.insert(entity, Dirty);
            true
        }
        _ => false,
    }
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
    View::new("Button", 40, button_render).with_attach(button_attach)
}
