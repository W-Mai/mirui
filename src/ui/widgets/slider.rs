use crate::ecs::{Entity, World};
use crate::input::event::gesture::{DragCancel, DragEnd, DragMove, DragStart, Tap};
use crate::render::command::DrawCommand;
use crate::types::{Fixed, Rect};
use crate::ui::ComputedRect;
use crate::ui::property::PropertyChange;
use crate::ui::theme::{ColorToken, ThemedColor};

crate::input::event::contract::widget_events! {
    Slider;
    ValueChanged { new: Fixed, old: Fixed },
    DragStarted,
    DragEnded,
    DragCancelled,
}

#[crate::component]
pub struct Slider {
    pub value: Fixed,
    pub min: Fixed,
    pub max: Fixed,
    pub track_color: ThemedColor,
    pub fill_color: ThemedColor,
    pub thumb_color: ThemedColor,
}

impl Default for Slider {
    fn default() -> Self {
        Self::new(Fixed::ZERO, Fixed::ONE)
    }
}

impl Slider {
    pub fn new(min: Fixed, max: Fixed) -> Self {
        Self {
            value: min,
            min,
            max,
            track_color: ThemedColor::Token(ColorToken::SurfaceVariant),
            fill_color: ThemedColor::Token(ColorToken::Primary),
            thumb_color: ThemedColor::Token(ColorToken::OnPrimary),
        }
    }

    pub fn with_track_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.track_color = color.into();
        self
    }

    pub fn with_fill_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.fill_color = color.into();
        self
    }

    pub fn with_thumb_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.thumb_color = color.into();
        self
    }

    pub fn ratio(&self) -> Fixed {
        let range = self.max - self.min;
        if range <= Fixed::ZERO {
            return Fixed::ZERO;
        }
        (self.value - self.min) / range
    }

    pub fn set_ratio(&mut self, ratio: Fixed) {
        self.update_value(self.value_from_ratio(ratio));
    }

    fn value_from_ratio(&self, ratio: Fixed) -> Fixed {
        let clamped = ratio.clamp(Fixed::ZERO, Fixed::ONE);
        self.min + clamped * (self.max - self.min)
    }

    pub(crate) fn update_value(&mut self, value: Fixed) -> PropertyChange {
        let old = self.value;
        let lower = self.min.min(self.max);
        let upper = self.min.max(self.max);
        let new = value.clamp(lower, upper);
        if old == new {
            PropertyChange::Unchanged
        } else {
            self.value = new;
            PropertyChange::Visual
        }
    }

    pub fn build(min: impl Into<Fixed>, max: impl Into<Fixed>) -> SliderBuilder {
        SliderBuilder {
            slider: Slider::new(min.into(), max.into()),
            style: None,
            properties: (),
            bindings: (),
        }
    }
}

pub struct SliderBuilder<P = (), B = ()> {
    slider: Slider,
    style: Option<crate::ui::Style>,
    properties: P,
    bindings: B,
}

impl<P, B> SliderBuilder<P, B> {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn track_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.slider.track_color = color.into();
        self
    }

    pub fn fill_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.slider.fill_color = color.into();
        self
    }

    pub fn thumb_color(mut self, color: impl Into<ThemedColor>) -> Self {
        self.slider.thumb_color = color.into();
        self
    }
}

crate::ui::recipe::widget_builder_bindings!(SliderBuilder, Slider, slider, style);

impl<P: crate::__macro_support::PropertyBindings, B: crate::__macro_support::EntityEventBindings>
    crate::ui::recipe::WidgetRecipe for SliderBuilder<P, B>
{
    type Properties = P;
    type Events = B;

    fn insert(self, world: &mut World, entity: Entity) -> (P, B) {
        world.insert(entity, self.slider);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        (self.properties, self.bindings)
    }
}

/// Right edge of the fill bar, measured from the track's left edge.
/// Held equal to `knob_center_offset` so the round knob always covers
/// the fill's square right edge.
fn fill_width(ratio: Fixed, track_w: Fixed, thumb_size: Fixed) -> Fixed {
    ratio * (track_w - thumb_size) + thumb_size / Fixed::from_int(2)
}

/// Knob centre, measured from the track's left edge. The knob travels
/// the usable track (full width minus its own diameter).
fn knob_center_offset(ratio: Fixed, track_w: Fixed, thumb_size: Fixed) -> Fixed {
    ratio * (track_w - thumb_size) + thumb_size / Fixed::from_int(2)
}

#[crate::view(
    component = Slider,
    name = "Slider",
    priority = 60,
    attach = slider_attach::callback(),
    input = (slider_tap::callback(), slider_drag::callback(), slider_start::callback(), slider_end::callback(), slider_cancel::callback())
)]
fn slider_render(component: component!(), paint: paint!(), theme: res!(crate::ui::Theme)) {
    let rect = paint.rect();
    let state = paint.state();
    let track_color = component.track_color.resolve_in(&theme, state);
    let fill_color = component.fill_color.resolve_in(&theme, state);
    let thumb_color = component.thumb_color.resolve_in(&theme, state);
    let ratio = component.ratio();
    let cap_radius = rect.h / Fixed::from_int(2);
    let thumb_size = rect.h;

    paint.draw(&DrawCommand::Fill {
        area: rect,
        transform: paint.transform(),
        quad: paint.quad(),
        color: track_color,
        radius: cap_radius,
        opa: 255,
    });

    // Capsule clipped to ratio_w; the square right edge sits under the knob.
    let ratio_w = fill_width(ratio, rect.w, thumb_size);
    if ratio_w > Fixed::ZERO {
        let ratio_box = Rect {
            x: rect.x,
            y: rect.y,
            w: ratio_w,
            h: rect.h,
        };
        let ratio_clip = paint.transform().apply_rect_bbox(ratio_box);
        if let Some(fill_clip) = paint.clip().intersect(&ratio_clip) {
            paint.draw_clipped(
                &DrawCommand::Fill {
                    area: rect,
                    transform: paint.transform(),
                    quad: paint.quad(),
                    color: fill_color,
                    radius: cap_radius,
                    opa: 255,
                },
                fill_clip,
            );
        }
    }

    let thumb_x =
        rect.x + knob_center_offset(ratio, rect.w, thumb_size) - thumb_size / Fixed::from_int(2);
    paint.draw(&DrawCommand::Fill {
        area: Rect {
            x: thumb_x,
            y: rect.y,
            w: thumb_size,
            h: thumb_size,
        },
        transform: paint.transform(),
        quad: paint.quad(),
        color: thumb_color,
        radius: thumb_size / Fixed::from_int(2),
        opa: 255,
    });
}

fn slider_value(slider: &Slider, rect: Rect, x: Fixed) -> Option<ValueChanged> {
    let ratio = (x - rect.x).max(Fixed::ZERO) / rect.w;
    let new = slider.value_from_ratio(ratio);
    (new != slider.value).then_some(ValueChanged {
        new,
        old: slider.value,
    })
}

#[crate::event]
fn slider_tap(
    node: entity!(),
    input: event!(Tap),
    sliders: query!(Slider, ComputedRect),
    commands: commands!(),
) -> bool {
    let Some((slider, rect)) = sliders.get(node) else {
        return false;
    };
    if rect.0.w <= Fixed::ZERO {
        return false;
    }
    if let Some(changed) = slider_value(slider, rect.0, input.x) {
        commands.set::<crate::ui::property::prop::SliderValue>(node, changed.new);
        commands.emit(node, changed);
    }
    true
}

#[crate::event]
fn slider_drag(
    node: entity!(),
    input: event!(DragMove),
    sliders: query!(Slider, ComputedRect),
    commands: commands!(),
) -> bool {
    let Some((slider, rect)) = sliders.get(node) else {
        return false;
    };
    if rect.0.w <= Fixed::ZERO {
        return false;
    }
    if let Some(changed) = slider_value(slider, rect.0, input.x) {
        commands.set::<crate::ui::property::prop::SliderValue>(node, changed.new);
        commands.emit(node, changed);
    }
    true
}

#[crate::event]
fn slider_start(node: entity!(), _input: event!(DragStart), commands: commands!()) {
    commands.emit(node, DragStarted);
}

#[crate::event]
fn slider_end(node: entity!(), _input: event!(DragEnd), commands: commands!()) {
    commands.emit(node, DragEnded);
}

#[crate::event]
fn slider_cancel(node: entity!(), _input: event!(DragCancel), commands: commands!()) {
    commands.emit(node, DragCancelled);
}

#[crate::event]
fn slider_attach(entity: entity!(), commands: commands!()) {
    commands.insert(entity, crate::input::event::scroll::TouchAction::PanY);
}

pub use slider_render::view;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::CommandError;
    use crate::input::event::gesture::GestureEvent;
    use crate::input::event::{EventCapacity, runtime};
    use crate::ui::dirty::VisualDirty;

    #[derive(Default)]
    struct EventLog {
        order: [Option<&'static str>; 4],
        len: usize,
        value: Option<ValueChanged>,
    }

    impl EventLog {
        fn push(&mut self, name: &'static str) {
            self.order[self.len] = Some(name);
            self.len += 1;
        }
    }

    #[crate::event]
    fn record_value(
        input: event!(ValueChanged),
        node: entity!(),
        values: query!(Slider, VisualDirty),
        trace: res!(mut EventLog),
    ) {
        assert!(values.get(node).is_some());
        trace.push("ValueChanged");
        trace.value = Some(*input);
    }

    #[crate::event]
    fn record_start(trace: res!(mut EventLog)) {
        trace.push("DragStarted");
    }

    #[crate::event]
    fn record_end(trace: res!(mut EventLog)) {
        trace.push("DragEnded");
    }

    #[crate::event]
    fn record_cancel(trace: res!(mut EventLog)) {
        trace.push("DragCancelled");
    }

    #[crate::event]
    fn record_second(input: event!(ValueChanged), trace: res!(mut EventLog)) {
        trace.push("second");
        trace.value = Some(*input);
    }

    #[crate::event]
    fn remove_slider(node: entity!(), values: query!(Slider, VisualDirty), commands: commands!()) {
        assert!(values.get(node).is_some());
        commands.despawn(node);
    }

    fn fresh() -> (World, Entity) {
        let mut world = World::new();
        world.insert_resource(EventLog::default());
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .commands(crate::input::event::view_input::fixture_capacity())
                .notifications::<ValueChanged>(2)
                .notifications::<DragStarted>(1)
                .notifications::<DragEnded>(1)
                .notifications::<DragCancelled>(1)
                .flush_limit(4),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent).spawn(
            Slider::build(Fixed::ZERO, Fixed::from_int(100))
                .on_event(Slider::ValueChanged, record_value::callback())
                .on_event(Slider::DragStarted, record_start::callback())
                .on_event(Slider::DragEnded, record_end::callback())
                .on_event(Slider::DragCancelled, record_cancel::callback()),
        );
        world.insert(entity, ComputedRect(Rect::new(0, 0, 100, 20)));
        (world, entity)
    }

    #[test]
    fn tap_emits_value_changed_with_new_old() {
        let (mut world, entity) = fresh();
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: Fixed::from_int(50),
                    y: Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert_eq!(world.resource::<EventLog>().unwrap().len, 0);
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 1);
        assert_eq!(
            world.resource::<EventLog>().unwrap().value,
            Some(ValueChanged {
                old: Fixed::ZERO,
                new: Fixed::from_int(50),
            })
        );
    }

    #[test]
    fn drag_lifecycle_notifies_in_fifo_order() {
        for cancelled in [false, true] {
            let (mut world, entity) = fresh();
            let terminal = if cancelled {
                GestureEvent::DragCancel {
                    x: 75.into(),
                    y: Fixed::ZERO,
                    target: entity,
                }
            } else {
                GestureEvent::DragEnd {
                    x: 75.into(),
                    y: Fixed::ZERO,
                    vx: Fixed::ZERO,
                    vy: Fixed::ZERO,
                    target: entity,
                }
            };
            for event in [
                GestureEvent::DragStart {
                    x: 40.into(),
                    y: Fixed::ZERO,
                    target: entity,
                },
                GestureEvent::DragMove {
                    x: 75.into(),
                    y: Fixed::ZERO,
                    dx: Fixed::ZERO,
                    dy: Fixed::ZERO,
                    target: entity,
                },
                terminal,
            ] {
                assert!(
                    crate::input::event::view_input::invoke_gesture(
                        view(),
                        &mut world,
                        entity,
                        &event
                    )
                    .unwrap()
                );
            }
            assert!(world.has::<VisualDirty>(entity));
            assert!(!world.has::<crate::ui::dirty::Dirty>(entity));
            assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 3);
            let trace = world.resource::<EventLog>().unwrap();
            assert_eq!(
                &trace.order[..trace.len],
                &[
                    Some("DragStarted"),
                    Some("ValueChanged"),
                    Some(if cancelled {
                        "DragCancelled"
                    } else {
                        "DragEnded"
                    })
                ]
            );
            assert_eq!(trace.value.unwrap().new, Fixed::from_int(75));
            assert_eq!(
                world.get::<Slider>(entity).unwrap().value,
                Fixed::from_int(75)
            );
        }
    }

    #[test]
    fn no_value_change_does_not_emit() {
        let (mut world, entity) = fresh();
        world.get_mut::<Slider>(entity).unwrap().value = Fixed::from_int(50);
        crate::input::event::view_input::fixture_inputs(view(), &mut world);
        world.remove::<VisualDirty>(entity);
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 50.into(),
                    y: Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
        assert_eq!(world.resource::<EventLog>().unwrap().len, 0);
        assert!(!world.has::<VisualDirty>(entity));
    }

    #[test]
    fn programmatic_value_uses_the_input_setter_without_emitting_an_event() {
        let (mut world, entity) = fresh();
        crate::input::event::view_input::fixture_inputs(view(), &mut world);
        assert_eq!(
            crate::ui::property::apply_to_world::<crate::ui::property::prop::SliderValue>(
                &mut world,
                entity,
                75.into(),
            ),
            PropertyChange::Visual,
        );
        assert_eq!(
            world.get::<Slider>(entity).unwrap().value,
            Fixed::from_int(75)
        );
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
        crate::ui::view::ViewRegistry::reconcile_observations(&mut world);
        world.remove::<VisualDirty>(entity);
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
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
        assert_eq!(world.resource::<EventLog>().unwrap().len, 0);
        assert!(!world.has::<VisualDirty>(entity));
    }

    #[test]
    fn value_callback_can_remove_the_slider_after_the_input_route() {
        let mut world = World::new();
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .notifications::<ValueChanged>(1)
                .commands(crate::input::event::view_input::fixture_capacity())
                .flush_limit(1),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent)
            .spawn(Slider::build(0, 100).on_event(Slider::ValueChanged, remove_slider::callback()));
        world.insert(entity, ComputedRect(Rect::new(0, 0, 100, 20)));
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 50.into(),
                    y: Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert!(world.is_alive(entity));
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 1);
        assert!(!world.is_alive(entity));
    }

    #[test]
    fn build_spawns_slider_with_style_and_typed_event_bindings() {
        let mut world = World::new();
        world.insert_resource(EventLog::default());
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
            Slider::build(0, 100)
                .on_event(Slider::ValueChanged, record_value::callback())
                .style(crate::ui::Style::default()),
        );
        world.insert(entity, ComputedRect(Rect::new(0, 0, 100, 20)));
        assert!(world.has::<crate::ui::Style>(entity));
        assert!(world.has::<crate::ui::Widget>(entity));
        crate::input::event::view_input::invoke_gesture(
            view(),
            &mut world,
            entity,
            &GestureEvent::Tap {
                x: 25.into(),
                y: Fixed::ZERO,
                target: entity,
            },
        )
        .unwrap();
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 1);
        assert_eq!(world.resource::<EventLog>().unwrap().len, 1);
    }

    #[test]
    fn builder_preserves_same_event_listener_order_and_fanout() {
        let mut world = World::new();
        world.insert_resource(EventLog::default());
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
            Slider::build(0, 100)
                .on_event(Slider::ValueChanged, record_second::callback())
                .on_event(Slider::ValueChanged, record_value::callback()),
        );
        world.insert(entity, ComputedRect(Rect::new(0, 0, 100, 20)));
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 50.into(),
                    y: Fixed::ZERO,
                    target: entity,
                },
            )
            .unwrap()
        );
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 1);
        let trace = world.resource::<EventLog>().unwrap();
        assert_eq!(
            &trace.order[..trace.len],
            &[Some("second"), Some("ValueChanged")]
        );
    }

    #[test]
    fn empty_builder_configuration_does_not_allocate_event_storage() {
        assert_eq!(
            crate::test_support::tracked_allocations(|| {
                let _ = Slider::build(0, 100).fill_color(ColorToken::Primary);
            }),
            0
        );
    }

    #[test]
    fn bare_builder_does_not_require_notification_storage() {
        let mut world = World::new();
        let builder_parent = world.spawn_empty();
        let entity = crate::ui::UiScope::new(&mut world, builder_parent).spawn(Slider::build(0, 1));
        world.insert(entity, ComputedRect(Rect::new(0, 0, 100, 20)));
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 100.into(),
                    y: Fixed::ZERO,
                    target: entity,
                }
            )
            .unwrap()
        );
        assert_eq!(world.get::<Slider>(entity).unwrap().value, Fixed::ONE);
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
        assert!(world.has::<crate::ui::Style>(entity));
    }

    #[test]
    fn lazy_notifications_require_the_exact_entity_and_payload_subscription() {
        let mut world = World::new();
        world.insert_resource(EventLog::default());
        runtime::configure(
            &mut world,
            EventCapacity::new()
                .commands(crate::input::event::view_input::fixture_capacity())
                .notifications::<DragStarted>(1)
                .flush_limit(1),
        )
        .unwrap();
        let builder_parent = world.spawn_empty();
        let subscribed = crate::ui::UiScope::new(&mut world, builder_parent)
            .spawn(Slider::build(0, 100).on_event(Slider::DragStarted, record_start::callback()));
        let builder_parent = world.spawn_empty();
        let other =
            crate::ui::UiScope::new(&mut world, builder_parent).spawn(Slider::build(0, 100));
        world.insert(subscribed, ComputedRect(Rect::new(0, 0, 100, 20)));
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                subscribed,
                &GestureEvent::Tap {
                    x: 50.into(),
                    y: Fixed::ZERO,
                    target: subscribed,
                },
            )
            .unwrap()
        );
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                other,
                &GestureEvent::DragStart {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                    target: other,
                },
            )
            .unwrap()
        );
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 0);
        assert_eq!(world.resource::<EventLog>().unwrap().len, 0);
        assert!(
            crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                subscribed,
                &GestureEvent::DragStart {
                    x: Fixed::ZERO,
                    y: Fixed::ZERO,
                    target: subscribed,
                },
            )
            .unwrap()
        );
        assert_eq!(crate::ui::delivery::flush(&mut world).unwrap(), 1);
        assert_eq!(world.resource::<EventLog>().unwrap().len, 1);
    }

    #[test]
    fn input_preserves_updates_when_notification_budget_is_missing_or_zero() {
        for (capacity, explicit_zero) in [
            (
                EventCapacity::new().commands(crate::input::event::view_input::fixture_capacity()),
                false,
            ),
            (
                EventCapacity::new()
                    .commands(crate::input::event::view_input::fixture_capacity())
                    .notifications::<ValueChanged>(0),
                true,
            ),
        ] {
            let mut world = World::new();
            world.insert_resource(EventLog::default());
            runtime::configure(&mut world, capacity).unwrap();
            let builder_parent = world.spawn_empty();
            let entity = crate::ui::UiScope::new(&mut world, builder_parent).spawn(
                Slider::build(0, 100).on_event(Slider::ValueChanged, record_value::callback()),
            );
            world.insert(entity, ComputedRect(Rect::new(0, 0, 100, 20)));
            let error = crate::input::event::view_input::invoke_gesture(
                view(),
                &mut world,
                entity,
                &GestureEvent::Tap {
                    x: 50.into(),
                    y: Fixed::ZERO,
                    target: entity,
                },
            );
            if explicit_zero {
                assert!(!runtime::has_entity_listener::<ValueChanged>(&world, entity).unwrap());
                assert_eq!(error, Ok(true));
            } else {
                assert!(matches!(
                    error,
                    Err(CommandError::Commit(
                        crate::ecs::CommandCommitError::MissingService { .. }
                    ))
                ));
            }
            assert_eq!(
                world.get::<Slider>(entity).unwrap().value,
                Fixed::from_int(50)
            );
            assert!(world.has::<VisualDirty>(entity));
            assert_eq!(world.resource::<EventLog>().unwrap().len, 0);
        }
    }

    #[test]
    fn fill_clip_follows_the_same_transform_as_the_track() {
        use crate::render::renderer::{DrawRequest, RenderError, Renderer};
        use crate::types::Transform;
        use crate::ui::theme::WidgetState;
        use crate::ui::view::PaintCtx;
        use crate::ui::{Style, Theme};

        struct Trace {
            clips: [Rect; 3],
            count: usize,
            transform: Transform,
        }
        impl Renderer for Trace {
            type TargetState = ();

            fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
                let DrawCommand::Fill { transform, .. } = request.command else {
                    panic!("expected slider fill")
                };
                assert_eq!(*transform, self.transform);
                self.clips[self.count] = request.clip;
                self.count += 1;
                Ok(())
            }
            fn flush(&mut self) -> Result<(), ::mirui::render::RenderError> {
                Ok(())
            }
        }

        let mut world = World::new();
        world.insert_resource(Theme::default());
        let node = world.spawn_empty();
        world.insert(
            node,
            Slider {
                value: Fixed::from_ratio(1, 2),
                ..Slider::default()
            },
        );
        let rect = Rect::new(20, 30, 100, 20);
        let clip = Rect::new(0, 0, 300, 200);
        let style = Style::default();
        let render = view().render().unwrap();
        for transform in [
            Transform::IDENTITY,
            Transform::translate(40.into(), 10.into()),
            Transform::translate(140.into(), 80.into())
                .compose(&Transform::scale((-1).into(), 2.into())),
        ] {
            let mut trace = Trace {
                clips: [Rect::ZERO; 3],
                count: 0,
                transform,
            };
            let mut paint = PaintCtx::new(
                &mut trace,
                &style,
                &rect,
                &clip,
                transform,
                None,
                WidgetState::Enabled,
            );
            let allocations = crate::test_support::tracked_allocations(|| {
                render(&world, node, &mut paint);
            });
            assert_eq!(paint.error(), None);
            assert_eq!(allocations, 0);
            assert_eq!(trace.count, 3);
            assert_eq!(
                trace.clips[1],
                transform
                    .apply_rect_bbox(Rect::new(20, 30, 50, 20))
                    .intersect(&clip)
                    .unwrap()
            );
        }
    }

    #[test]
    fn fill_edge_lands_on_knob_center() {
        let track_w = Fixed::from_int(200);
        let thumb_size = Fixed::from_int(20);
        for ratio in [Fixed::ZERO, Fixed::from_f32(0.5), Fixed::ONE] {
            assert_eq!(
                fill_width(ratio, track_w, thumb_size),
                knob_center_offset(ratio, track_w, thumb_size)
            );
        }
    }

    #[test]
    fn fill_width_never_zero_at_min() {
        assert_eq!(
            fill_width(Fixed::ZERO, 200.into(), 20.into()),
            Fixed::from_int(10)
        );
    }

    #[test]
    fn typed_view_keeps_slider_registration_metadata() {
        let view = view();
        assert_eq!(view.name(), "Slider");
        assert_eq!(view.priority(), Some(60));
        assert!(view.auto_attach().is_some());
        assert!(view.inputs().is_some());
    }

    #[test]
    fn attach_keeps_horizontal_drag_for_the_slider() {
        let mut world = World::new();
        let entity = world.spawn_empty();
        view().auto_attach().unwrap().invoke(&mut world, entity);
        assert_eq!(
            world.get::<crate::input::event::scroll::TouchAction>(entity),
            Some(&crate::input::event::scroll::TouchAction::PanY)
        );
    }
}
