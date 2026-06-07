use crate::draw::command::DrawCommand;
use crate::draw::renderer::Renderer;
use crate::ecs::{Entity, World};
use crate::event::GestureHandler;
use crate::event::gesture::GestureEvent;
use crate::types::{Fixed, Rect};
use crate::widget::ComputedRect;
use crate::widget::dirty::Dirty;
use crate::widget::theme::{ColorToken, ThemedColor};
use crate::widget::view::{View, ViewCtx};

/// Horizontal tab bar with N children laid out flex-row.
/// `selected` is the discrete tab index; `indicator_offset` is the
/// continuous (0.0 .. count) position the renderer reads.
pub struct TabBar {
    pub selected: u8,
    pub count: u8,
    pub indicator_offset: Fixed,
    pub indicator_color: ThemedColor,
    pub indicator_height: Fixed,
}

impl Default for TabBar {
    fn default() -> Self {
        Self::new(0)
    }
}

impl TabBar {
    pub fn new(count: u8) -> Self {
        Self {
            selected: 0,
            count,
            indicator_offset: Fixed::ZERO,
            indicator_color: ThemedColor::Token(ColorToken::Primary),
            indicator_height: Fixed::from_int(2),
        }
    }

    pub fn with_indicator_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.indicator_color = color.into();
        self
    }

    pub fn with_indicator_height(mut self, height: impl Into<Fixed>) -> Self {
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
    let theme = ctx.theme(world);
    let indicator_color = tb.indicator_color.resolve_in(theme, ctx.state);
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
            color: indicator_color,
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
    View::new("TabBar", 60, tab_bar_render)
        .with_filter::<TabBar>()
        .with_attach(tab_bar_attach)
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
        use crate::types::Color;
        let tb = TabBar::new(3)
            .with_indicator_color(Color::rgb(255, 0, 0))
            .with_indicator_height(5);
        assert_eq!(tb.indicator_color, ThemedColor::Raw(Color::rgb(255, 0, 0)));
        assert_eq!(tb.indicator_height, Fixed::from_int(5));
    }

    #[test]
    fn with_indicator_token_via_into() {
        let tb = TabBar::new(3).with_indicator_color(ColorToken::Success);
        assert_eq!(tb.indicator_color, ThemedColor::Token(ColorToken::Success),);
    }
}
