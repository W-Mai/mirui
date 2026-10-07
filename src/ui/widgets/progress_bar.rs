use crate::__macro_support::{
    EntityContext, EntityEventBindings, EntityInstallRoute, EventBinding, EventCallback,
};
use crate::ecs::{CommandError, Entity, World};
use crate::input::event::contract::{EntitySourceFor, Event, EventKey, EventRoute};
use crate::input::event::gesture::GestureEvent;
use crate::render::command::DrawCommand;
use crate::types::{Fixed, Rect};
use crate::ui::ComputedRect;
use crate::ui::dirty::Dirty;
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
            bindings: (),
        }
    }
}

pub struct ProgressBarBuilder<B = ()> {
    progress_bar: ProgressBar,
    style: Option<crate::ui::Style>,
    bindings: B,
}

impl<B> ProgressBarBuilder<B> {
    pub fn style(mut self, style: crate::ui::Style) -> Self {
        self.style = Some(style);
        self
    }

    pub fn on_event<E>(
        self,
        key: EventKey<E>,
        callback: EventCallback<E, EntityContext<ProgressBar>>,
    ) -> ProgressBarBuilder<(B, EventBinding<E, EntityContext<ProgressBar>>)>
    where
        E: Event,
        E::Route: EntityInstallRoute<E>,
        <E::Route as EventRoute>::Decision: crate::__macro_support::ListenerDecision,
        E::Source: EntitySourceFor<ProgressBar>,
    {
        ProgressBarBuilder {
            progress_bar: self.progress_bar,
            style: self.style,
            bindings: (self.bindings, EventBinding::new(key, callback)),
        }
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

    pub fn spawn(self, world: &mut World) -> Entity
    where
        B: EntityEventBindings,
    {
        world.spawn(self)
    }
}

impl<B: EntityEventBindings> crate::ecs::IntoBundle for ProgressBarBuilder<B> {
    fn spawn_into(self, world: &mut World, entity: Entity) {
        world.insert(entity, self.progress_bar);
        if let Some(style) = self.style {
            world.insert(entity, style);
        }
        self.bindings.install(world, entity);
    }
}

#[crate::view(component = ProgressBar, name = "ProgressBar", priority = 60, gesture = progress_bar_handler)]
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

/// Map pointer x onto the bar's ComputedRect to drive `value` in
/// [0, 1]. Both Tap and DragMove route here so dragging produces a
/// continuous update.
fn progress_bar_handler(
    world: &mut World,
    entity: Entity,
    event: &GestureEvent,
) -> Result<bool, CommandError> {
    let x = match event {
        GestureEvent::Tap { x, .. } | GestureEvent::DragMove { x, .. } => *x,
        _ => return Ok(false),
    };
    let Some(rect) = world.get::<ComputedRect>(entity).map(|c| c.0) else {
        return Ok(false);
    };
    if rect.w <= Fixed::ZERO {
        return Ok(false);
    }
    let ratio = ((x - rect.x).to_f32() / rect.w.to_f32()).clamp(0.0, 1.0);
    let (old_value, new_value) = if let Some(pb) = world.get_mut::<ProgressBar>(entity) {
        let old = pb.value;
        pb.value = ratio;
        (old, pb.value)
    } else {
        return Ok(false);
    };
    world.insert(entity, Dirty);
    if old_value != new_value {
        crate::input::event::runtime::emit_widget(
            world,
            entity,
            ValueChanged {
                new: new_value,
                old: old_value,
            },
        )?;
    }
    Ok(true)
}

pub use progress_bar_render::view;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::event::{EventCapacity, runtime};

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
                .notifications::<ValueChanged>(1)
                .flush_limit(1),
        )
        .unwrap();
        let entity = ProgressBar::build()
            .on_event(ProgressBar::ValueChanged, record_value::callback())
            .style(crate::ui::Style::default())
            .spawn(&mut world);
        world.insert(entity, crate::ui::ComputedRect(Rect::new(0, 0, 100, 20)));
        assert!(world.has::<crate::ui::Style>(entity));
        assert!(world.has::<crate::ui::Widget>(entity));
        assert!(
            progress_bar_handler(
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
        assert!(world.has::<Dirty>(entity));
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
            progress_bar_handler(
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
        let entity = ProgressBar::build().spawn(&mut world);
        world.insert(entity, crate::ui::ComputedRect(Rect::new(0, 0, 100, 20)));
        assert!(!world.has::<crate::ui::Style>(entity));
        assert!(
            progress_bar_handler(
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
