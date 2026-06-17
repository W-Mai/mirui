use crate::ecs::{Entity, World};
use crate::event::gesture::GestureEvent;
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::Rect;
use crate::widget::dirty::Dirty;
use crate::widget::theme::{ColorToken, ThemedColor};
use crate::widget::view::{View, ViewCtx};

#[derive(crate::Component)]
pub struct Button {
    pub pressed: bool,
    pub normal_color: ThemedColor,
    pub pressed_color: ThemedColor,
}

impl Default for Button {
    fn default() -> Self {
        Self {
            pressed: false,
            normal_color: ThemedColor::Token(ColorToken::SurfaceVariant),
            pressed_color: ThemedColor::Token(ColorToken::Primary),
        }
    }
}

impl Button {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_normal_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.normal_color = color.into();
        self
    }

    pub fn with_pressed_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.pressed_color = color.into();
        self
    }

    pub fn build() -> ButtonBuilder {
        ButtonBuilder {
            button: Button::new(),
            style: None,
        }
    }
}

pub struct ButtonBuilder {
    button: Button,
    style: Option<crate::widget::Style>,
}

impl ButtonBuilder {
    pub fn style(mut self, style: crate::widget::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn normal_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.button.normal_color = color.into();
        self
    }

    pub fn pressed_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.button.pressed_color = color.into();
        self
    }

    pub fn spawn(self, world: &mut World) -> Entity {
        world.spawn(self)
    }
}

impl crate::ecs::IntoBundle for ButtonBuilder {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.button);
        if let Some(style) = self.style {
            world.insert(entity, style);
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
    let theme = ctx.theme(world);
    let color = if btn.pressed {
        btn.pressed_color.resolve_in(theme, ctx.state)
    } else {
        btn.normal_color.resolve_in(theme, ctx.state)
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
            false
        }
        _ => false,
    }
}

fn button_attach(world: &mut World, entity: Entity) {
    let _ = world;
    let _ = entity;
}

pub fn view() -> View {
    View::new("Button", 40, button_render)
        .with_filter::<Button>()
        .with_attach(button_attach)
        .with_internal_gesture(button_handler)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_spawns_button_with_style() {
        let mut world = World::new();
        let e = Button::build()
            .style(crate::widget::Style::default())
            .spawn(&mut world);
        assert!(world.has::<Button>(e));
        assert!(world.has::<crate::widget::Style>(e));
        assert!(world.has::<crate::widget::Widget>(e));
    }

    #[test]
    fn build_without_style_omits_it() {
        let mut world = World::new();
        let e = Button::build().spawn(&mut world);
        assert!(world.has::<Button>(e));
        assert!(!world.has::<crate::widget::Style>(e));
    }
}
