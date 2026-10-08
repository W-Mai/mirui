use crate::anim::Tween;
use crate::ecs::{CommandError, Entity, World};
use crate::input::event::gesture::GestureEvent;
use crate::render::command::DrawCommand;
use crate::types::{Fixed, Rect};
use crate::ui::ComputedRect;
use crate::ui::dirty::Dirty;
use crate::ui::theme::{ColorToken, ThemedColor};

crate::input::event::contract::widget_events! {
    TabBar;
    SelectionChanged { new: u8, old: u8 },
}

pub(crate) const INDICATOR_TWEEN_MS: u16 = 220;

pub(crate) struct TabIndicatorTween {
    pub(crate) tween: Tween,
}

impl crate::ecs::Component for TabIndicatorTween {}

pub(crate) struct TabBarPrev {
    pub(crate) selected: u8,
}

impl crate::ecs::Component for TabBarPrev {}

/// Horizontal tab bar with N children laid out flex-row.
/// `selected` is the discrete tab index; `indicator_offset` is the
/// continuous (0.0 .. count) position the renderer reads.
#[crate::component]
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
            properties: (),
            bindings: (),
        }
    }
}

pub struct TabBarBuilder<P = (), B = ()> {
    tab_bar: TabBar,
    style: Option<crate::ui::Style>,
    properties: P,
    bindings: B,
}

impl<P, B> TabBarBuilder<P, B> {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
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
}

crate::ui::recipe::widget_builder_bindings!(TabBarBuilder, TabBar, tab_bar, style);

impl<P: crate::__macro_support::PropertyBindings, B: crate::__macro_support::EntityEventBindings>
    crate::ui::recipe::WidgetRecipe for TabBarBuilder<P, B>
{
    type Properties = P;
    type Events = B;

    fn insert(self, world: &mut World, entity: Entity) -> (P, B) {
        world.insert(entity, self.tab_bar);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        (self.properties, self.bindings)
    }
}

#[crate::view(component = TabBar, name = "TabBar", priority = 60, gesture = tabbar_handler)]
fn tab_bar_render(tb: component!(), theme: res!(crate::ui::Theme), paint: paint!()) {
    if tb.count == 0 {
        return;
    }
    let state = paint.state();
    let rect = paint.rect();
    let indicator_color = tb.indicator_color.resolve_in(theme, state);
    let tab_w = rect.w / Fixed::from_int(tb.count as i32);
    let indicator_x = rect.x + tb.indicator_offset * tab_w;
    let indicator_y = rect.y + rect.h - tb.indicator_height;
    paint.draw(&DrawCommand::Fill {
        area: Rect {
            x: indicator_x,
            y: indicator_y,
            w: tab_w,
            h: tb.indicator_height,
        },
        transform: paint.transform(),
        quad: paint.quad(),
        color: indicator_color,
        radius: Fixed::ZERO,
        opa: 255,
    });
}

/// Tap on a tab: clamp pointer x onto the tab grid, update `selected`,
/// emit `SelectionChanged`, and start the indicator tween toward the
/// new index. The tween itself is driven by `tab_pages_system`.
pub(crate) fn tabbar_handler(
    world: &mut World,
    entity: Entity,
    event: &GestureEvent,
) -> Result<bool, CommandError> {
    let x = match event {
        GestureEvent::Tap { x, .. } => *x,
        _ => return Ok(false),
    };
    let Some(rect) = world.get::<ComputedRect>(entity).map(|c| c.0) else {
        return Ok(false);
    };
    if rect.w <= Fixed::ZERO {
        return Ok(false);
    }
    let count = match world.get::<TabBar>(entity) {
        Some(tb) if tb.count > 0 => tb.count,
        _ => return Ok(false),
    };
    let local = (x - rect.x).max(Fixed::ZERO);
    let tab_w = rect.w / Fixed::from_int(count as i32);
    let idx = (local / tab_w).to_int().clamp(0, count as i32 - 1) as u8;

    let (old_idx, current_offset) = match world.get::<TabBar>(entity) {
        Some(tb) => (tb.selected, tb.indicator_offset),
        None => return Ok(false),
    };
    if old_idx == idx {
        return Ok(true);
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
    world.insert(entity, Dirty);
    crate::input::event::runtime::emit_widget(
        world,
        entity,
        SelectionChanged {
            new: idx,
            old: old_idx,
        },
    )?;
    Ok(true)
}

pub use tab_bar_render::view;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::{EventCapacity, runtime};

    #[derive(Default)]
    struct SelectionTrace(Option<SelectionChanged>);

    #[crate::event]
    fn record_selection(input: event!(SelectionChanged), selected: res!(mut SelectionTrace)) {
        selected.0 = Some(*input);
    }

    #[crate::event]
    fn remove_bar(node: entity!(), bars: query!(TabBar, Dirty), commands: commands!()) {
        assert!(bars.get(node).is_some());
        commands.despawn(node);
    }

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
    fn builder_inserts_style_and_typed_event_binding() {
        let mut world = World::new();
        world.insert_resource(SelectionTrace::default());
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .notifications::<SelectionChanged>(1)
                .flush_limit(1),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent).spawn(
            TabBar::build(3)
                .on_event(TabBar::SelectionChanged, record_selection::callback())
                .style(crate::ui::Style::default()),
        );
        world.insert(entity, ComputedRect(Rect::new(0, 0, 90, 20)));
        assert!(world.has::<crate::ui::Style>(entity));
        assert!(world.has::<crate::ui::Widget>(entity));
        assert!(
            tabbar_handler(
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 45.into(),
                    y: Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert_eq!(world.get::<TabBar>(entity).unwrap().selected, 1);
        assert!(world.has::<TabIndicatorTween>(entity));
        assert!(world.has::<Dirty>(entity));
        assert_eq!(world.resource::<SelectionTrace>().unwrap().0, None);
        assert_eq!(runtime::flush(&mut world).unwrap(), 1);
        assert_eq!(
            world.resource::<SelectionTrace>().unwrap().0,
            Some(SelectionChanged { old: 0, new: 1 })
        );
        world.resource_mut::<SelectionTrace>().unwrap().0 = None;
        assert!(
            tabbar_handler(
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 45.into(),
                    y: Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert_eq!(runtime::flush(&mut world).unwrap(), 0);
        assert_eq!(world.resource::<SelectionTrace>().unwrap().0, None);
    }

    #[test]
    fn bare_builder_does_not_require_notification_storage() {
        let mut world = World::new();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent).spawn(TabBar::build(3));
        world.insert(entity, ComputedRect(Rect::new(0, 0, 90, 20)));
        assert!(world.has::<crate::ui::Style>(entity));
        assert!(
            tabbar_handler(
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 45.into(),
                    y: Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert_eq!(world.get::<TabBar>(entity).unwrap().selected, 1);
        assert_eq!(runtime::flush(&mut world).unwrap(), 0);
    }

    #[test]
    fn selection_callback_can_remove_tab_bar_after_the_input_route() {
        let mut world = World::new();
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .notifications::<SelectionChanged>(1)
                .commands(crate::ecs::CommandCapacity::new().despawn(1))
                .flush_limit(1),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent)
            .spawn(TabBar::build(3).on_event(TabBar::SelectionChanged, remove_bar::callback()));
        world.insert(entity, ComputedRect(Rect::new(0, 0, 90, 20)));
        assert!(
            tabbar_handler(
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 45.into(),
                    y: Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert!(world.is_alive(entity));
        assert_eq!(runtime::flush(&mut world).unwrap(), 1);
        assert!(!world.is_alive(entity));
    }
}
