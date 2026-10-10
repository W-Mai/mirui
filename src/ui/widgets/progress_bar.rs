use crate::ecs::{Entity, World};
use crate::input::event::gesture::{DragMove, Tap};
use crate::render::command::DrawCommand;
use crate::types::{Fixed, Rect};
use crate::ui::ComputedRect;
use crate::ui::theme::{ColorToken, ThemedColor};

crate::input::event::contract::widget_events! {
    ProgressBar;
    ValueChanged { new: f32, old: f32 },
}

#[crate::component]
pub struct ProgressBar {
    pub value: f32, // 0.0 ~ 1.0
    pub track_color: ThemedColor,
    pub fill_color: ThemedColor,
}

impl Default for ProgressBar {
    fn default() -> Self {
        Self {
            value: 0.0,
            track_color: ThemedColor::Token(ColorToken::SurfaceVariant),
            fill_color: ThemedColor::Token(ColorToken::Primary),
        }
    }
}

impl ProgressBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_track_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.track_color = color.into();
        self
    }

    pub fn with_fill_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.fill_color = color.into();
        self
    }

    pub fn build() -> ProgressBarBuilder {
        ProgressBarBuilder {
            progress_bar: ProgressBar::new(),
            style: None,
            properties: (),
            bindings: (),
        }
    }
}

pub struct ProgressBarBuilder<P = (), B = ()> {
    progress_bar: ProgressBar,
    style: Option<crate::ui::Style>,
    properties: P,
    bindings: B,
}

impl<P, B> ProgressBarBuilder<P, B> {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn value(mut self, v: f32) -> Self {
        self.progress_bar.value = v;
        self
    }

    pub fn track_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.progress_bar.track_color = color.into();
        self
    }

    pub fn fill_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.progress_bar.fill_color = color.into();
        self
    }
}

crate::ui::recipe::widget_builder_bindings!(ProgressBarBuilder, ProgressBar, progress_bar, style);

impl<P: crate::__macro_support::PropertyBindings, B: crate::__macro_support::EntityEventBindings>
    crate::ui::recipe::WidgetRecipe for ProgressBarBuilder<P, B>
{
    type Properties = P;
    type Events = B;

    fn insert(self, world: &mut World, entity: Entity) -> (P, B) {
        world.insert(entity, self.progress_bar);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        (self.properties, self.bindings)
    }
}

#[crate::view(component = ProgressBar, name = "ProgressBar", priority = 60, input = (progress_tap::callback(), progress_drag::callback()))]
fn progress_bar_render(pb: component!(), theme: res!(crate::ui::Theme), paint: paint!()) {
    let state = paint.state();
    let rect = paint.rect();
    let track_color = pb.track_color.resolve_in(theme, state);
    let fill_color = pb.fill_color.resolve_in(theme, state);
    paint.draw(&DrawCommand::Fill {
        area: rect,
        transform: paint.transform(),
        quad: paint.quad(),
        color: track_color,
        radius: paint.style().border_radius,
        opa: 255,
    });
    let fill_w = Fixed::from_f32(rect.w.to_f32() * pb.value.clamp(0.0, 1.0));
    if fill_w > Fixed::ZERO {
        paint.draw(&DrawCommand::Fill {
            area: Rect {
                x: rect.x,
                y: rect.y,
                w: fill_w,
                h: rect.h,
            },
            transform: paint.transform(),
            quad: paint.quad(),
            color: fill_color,
            radius: paint.style().border_radius,
            opa: 255,
        });
    }
}

fn set_progress(bar: &mut ProgressBar, rect: Rect, x: Fixed) -> Option<ValueChanged> {
    let new = ((x - rect.x).to_f32() / rect.w.to_f32()).clamp(0.0, 1.0);
    let old = bar.value;
    bar.value = new;
    (old != new).then_some(ValueChanged { old, new })
}

#[crate::event]
fn progress_tap(
    node: entity!(),
    input: event!(Tap),
    bars: query!(mut ProgressBar, ComputedRect),
    commands: commands!(),
) -> bool {
    let Some((bar, rect)) = bars.get(node) else {
        return false;
    };
    if rect.0.w <= Fixed::ZERO {
        return false;
    }
    if let Some(change) = set_progress(bar, rect.0, input.x) {
        commands.emit(node, change);
    }
    true
}

#[crate::event]
fn progress_drag(
    node: entity!(),
    input: event!(DragMove),
    bars: query!(mut ProgressBar, ComputedRect),
    commands: commands!(),
) -> bool {
    let Some((bar, rect)) = bars.get(node) else {
        return false;
    };
    if rect.0.w <= Fixed::ZERO {
        return false;
    }
    if let Some(change) = set_progress(bar, rect.0, input.x) {
        commands.emit(node, change);
    }
    true
}

pub use progress_bar_render::view;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::gesture::GestureEvent;
    use crate::input::event::{EventCapacity, runtime};
    use crate::ui::dirty::VisualDirty;

    #[derive(Default)]
    struct ValueTrace(Option<ValueChanged>);

    #[crate::event]
    fn record_value(input: event!(ValueChanged), values: res!(mut ValueTrace)) {
        values.0 = Some(*input);
    }

    #[test]
    fn build_spawns_progress_bar_with_style_and_typed_event_binding() {
        let mut world = World::new();
        world.insert_resource(ValueTrace::default());
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .commands(crate::input::event::view_input::fixture_capacity())
                .notifications::<ValueChanged>(1)
                .flush_limit(1),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent).spawn(
            ProgressBar::build()
                .on_event(ProgressBar::ValueChanged, record_value::callback())
                .style(crate::ui::Style::default()),
        );
        world.insert(entity, crate::ui::ComputedRect(Rect::new(0, 0, 100, 20)));
        assert!(world.has::<crate::ui::Style>(entity));
        assert!(world.has::<crate::ui::Widget>(entity));
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 75.into(),
                    y: Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert!(world.has::<VisualDirty>(entity));
        assert_eq!(world.resource::<ValueTrace>().unwrap().0, None);
        assert_eq!(runtime::flush(&mut world).unwrap(), 1);
        assert_eq!(
            world.resource::<ValueTrace>().unwrap().0,
            Some(ValueChanged {
                old: 0.0,
                new: 0.75
            })
        );
        world.resource_mut::<ValueTrace>().unwrap().0 = None;
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 75.into(),
                    y: Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert_eq!(runtime::flush(&mut world).unwrap(), 0);
        assert_eq!(world.resource::<ValueTrace>().unwrap().0, None);
    }

    #[test]
    fn bare_builder_does_not_require_notification_storage() {
        let mut world = World::new();
        let builder_parent = world.spawn_empty();
        let entity =
            crate::ui::UiScope::new(&mut world, builder_parent).spawn(ProgressBar::build());
        world.insert(entity, crate::ui::ComputedRect(Rect::new(0, 0, 100, 20)));
        assert!(world.has::<crate::ui::Style>(entity));
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 150.into(),
                    y: Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert_eq!(world.get::<ProgressBar>(entity).unwrap().value, 1.0);
        assert_eq!(runtime::flush(&mut world).unwrap(), 0);
    }
}
