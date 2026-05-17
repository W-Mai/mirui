use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
use crate::types::{Color, Fixed, Rect};
use crate::widget::ComputedRect;
use crate::widget::dirty::Dirty;
use crate::widget::view::{View, ViewCtx};

/// Horizontal tab bar with N children laid out flex-row.
/// `selected` is the discrete tab index; `indicator_offset` is the
/// continuous (0.0 .. count) position the renderer reads.
pub struct TabBar {
    pub selected: u8,
    pub count: u8,
    pub indicator_offset: Fixed,
    pub indicator_color: Color,
    pub indicator_height: Fixed,
}

impl TabBar {
    pub fn new(count: u8) -> Self {
        Self {
            selected: 0,
            count,
            indicator_offset: Fixed::ZERO,
            indicator_color: Color::rgb(88, 166, 255),
            indicator_height: Fixed::from_int(2),
        }
    }

    pub fn with_indicator(mut self, color: Color, height: impl Into<Fixed>) -> Self {
        self.indicator_color = color;
        self.indicator_height = height.into();
        self
    }
}

fn tab_bar_render(
    renderer: &mut dyn Renderer,
    world: &World,
    entity: Entity,
    rect: &Rect,
    ctx: &mut ViewCtx,
) {
    let Some(tb) = world.get::<TabBar>(entity) else {
        return;
    };
    if tb.count == 0 {
        return;
    }
    let tab_w = rect.w / Fixed::from_int(tb.count as i32);
    let indicator_x = rect.x + tb.indicator_offset * tab_w;
    let indicator_y = rect.y + rect.h - tb.indicator_height;
    renderer.draw(
        &DrawCommand::Fill {
            area: Rect {
                x: indicator_x,
                y: indicator_y,
                w: tab_w,
                h: tb.indicator_height,
            },
            transform: ctx.transform,
            quad: ctx.quad,
            color: tb.indicator_color,
            radius: Fixed::ZERO,
            opa: 255,
        },
        ctx.clip,
    );
}

/// Snap selected and indicator_offset to the tapped tab.
pub(crate) fn tabbar_handler(world: &mut World, entity: Entity, event: &GestureEvent) -> bool {
    let x = match event {
        GestureEvent::Tap { x, .. } => *x,
        _ => return false,
    };
    let Some(rect) = world.get::<ComputedRect>(entity).map(|c| c.0) else {
        return false;
    };
    if rect.w <= Fixed::ZERO {
        return false;
    }
    let count = match world.get::<TabBar>(entity) {
        Some(tb) if tb.count > 0 => tb.count,
        _ => return false,
    };
    let local = (x - rect.x).max(Fixed::ZERO);
    let tab_w = rect.w / Fixed::from_int(count as i32);
    let idx = (local / tab_w).to_int().clamp(0, count as i32 - 1) as u8;
    if let Some(tb) = world.get_mut::<TabBar>(entity) {
        tb.selected = idx;
        tb.indicator_offset = Fixed::from_int(idx as i32);
    }
    world.insert(entity, Dirty);
    true
}

fn tab_bar_attach(world: &mut World, entity: Entity) {
    if world.get::<TabBar>(entity).is_none() {
        return;
    }
    if world.get::<GestureHandler>(entity).is_some() {
        return;
    }
    world.insert(
        entity,
        GestureHandler {
            on_gesture: tabbar_handler,
        },
    );
}

pub fn view() -> View {
    View {
        name: "TabBar",
        priority: 60,
        render: tab_bar_render,
        auto_attach: Some(tab_bar_attach),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let tb = TabBar::new(4);
        assert_eq!(tb.count, 4);
        assert_eq!(tb.selected, 0);
        assert_eq!(tb.indicator_offset, Fixed::ZERO);
    }

    #[test]
    fn with_indicator_overrides() {
        let tb = TabBar::new(3).with_indicator(Color::rgb(255, 0, 0), 5);
        assert_eq!(tb.indicator_color.r, 255);
        assert_eq!(tb.indicator_height, Fixed::from_int(5));
    }
}
