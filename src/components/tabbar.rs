use crate::anim::Tween;
use crate::ecs::{Entity, World};
use crate::event::BusinessCallback;
use crate::event::gesture::GestureEvent;
use crate::render::command::DrawCommand;
use crate::render::renderer::Renderer;
use crate::types::{Fixed, Rect};
use crate::widget::ComputedRect;
use crate::widget::dirty::Dirty;
use crate::widget::theme::{ColorToken, ThemedColor};
use crate::widget::view::{View, ViewCtx};

#[derive(Clone, Debug)]
pub enum TabBarEvent {
    SelectionChanged { new: u8, old: u8 },
}

pub struct TabBarHandler {
    pub on_event: BusinessCallback<TabBarEvent>,
}

pub(crate) const INDICATOR_TWEEN_MS: u16 = 220;

pub(crate) struct TabIndicatorTween {
    pub(crate) tween: Tween,
}

pub(crate) struct TabBarPrev {
    pub(crate) selected: u8,
}

/// Horizontal tab bar with N children laid out flex-row.
/// `selected` is the discrete tab index; `indicator_offset` is the
/// continuous (0.0 .. count) position the renderer reads.
#[derive(crate::Component)]
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

    pub fn build(count: u8) -> TabBarBuilder {
        TabBarBuilder {
            tab_bar: TabBar::new(count),
            style: None,
            handler: None,
        }
    }
}

pub struct TabBarBuilder {
    tab_bar: TabBar,
    style: Option<crate::widget::Style>,
    handler: Option<TabBarHandler>,
}

impl TabBarBuilder {
    pub fn style(mut self, style: crate::widget::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn on_change(mut self, on_event: fn(&mut World, Entity, &TabBarEvent) -> bool) -> Self {
        self.handler = Some(TabBarHandler {
            on_event: BusinessCallback::Fn(on_event),
        });
        self
    }

    pub fn indicator_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.tab_bar.indicator_color = color.into();
        self
    }

    pub fn indicator_height(mut self, height: impl Into<Fixed>) -> Self {
        self.tab_bar.indicator_height = height.into();
        self
    }

    pub fn spawn(self, world: &mut World) -> Entity {
        world.spawn(self)
    }
}

impl crate::ecs::IntoBundle for TabBarBuilder {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.tab_bar);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        if let Some(handler) = self.handler {
            world.insert(entity, handler);
        }
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

/// Tap on a tab: clamp pointer x onto the tab grid, update `selected`,
/// emit `SelectionChanged`, and start the indicator tween toward the
/// new index. The tween itself is driven by `tab_pages_system`.
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

    let (old_idx, current_offset) = match world.get::<TabBar>(entity) {
        Some(tb) => (tb.selected, tb.indicator_offset),
        None => return false,
    };
    if old_idx == idx {
        return true;
    }
    if let Some(tb) = world.get_mut::<TabBar>(entity) {
        tb.selected = idx;
    }
    let to = Fixed::from_int(idx as i32);
    world.insert(
        entity,
        TabIndicatorTween {
            tween: Tween::ease_to(current_offset, to, INDICATOR_TWEEN_MS),
        },
    );
    world.insert(entity, TabBarPrev { selected: idx });
    emit_tabbar_event(
        world,
        entity,
        &TabBarEvent::SelectionChanged {
            new: idx,
            old: old_idx,
        },
    );
    world.insert(entity, Dirty);
    true
}

fn emit_tabbar_event(world: &mut World, entity: Entity, event: &TabBarEvent) {
    let cb = world
        .get::<TabBarHandler>(entity)
        .map(|h| h.on_event.clone_out());
    if let Some(cb) = cb {
        cb.call(world, entity, event);
    }
}

fn tab_bar_attach(world: &mut World, entity: Entity) {
    let _ = world;
    let _ = entity;
}

pub fn view() -> View {
    View::new("TabBar", 60, tab_bar_render)
        .with_filter::<TabBar>()
        .with_attach(tab_bar_attach)
        .with_internal_gesture(tabbar_handler)
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

    #[test]
    fn builder_inserts_all() {
        fn h(_: &mut World, _: Entity, _: &TabBarEvent) -> bool {
            true
        }
        let mut world = World::new();
        let e = TabBar::build(3)
            .style(crate::widget::Style::default())
            .on_change(h)
            .spawn(&mut world);
        assert!(world.get::<TabBar>(e).is_some());
        assert!(world.get::<crate::widget::Style>(e).is_some());
        assert!(world.get::<TabBarHandler>(e).is_some());
        assert!(world.get::<crate::widget::Widget>(e).is_some());
    }

    #[test]
    fn bare_builder_omits_optionals() {
        let mut world = World::new();
        let e = TabBar::build(3).spawn(&mut world);
        assert!(world.get::<TabBar>(e).is_some());
        assert!(world.get::<crate::widget::Style>(e).is_none());
        assert!(world.get::<TabBarHandler>(e).is_none());
    }
}
