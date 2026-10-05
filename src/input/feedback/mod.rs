//! Cursor + rotary input feedback overlays.
//!
//! Two ECS entities live under [`crate::ui::WidgetRoot`], one per
//! overlay kind. They are spawned by [`InputFeedbackPlugin`](crate::app::plugins::InputFeedbackPlugin) (cursor
//! lazily, on first [`crate::input::event::PointerCursor`]) and dirty-tracked
//! through the standard per-entity `Dirty` + `PrevRect` mechanism.
//!
//! State the systems read/write lives in a single [`InputFeedback`]
//! resource; per-overlay layout (cursor follows the pointer, rotary
//! pins to the right edge) is the entity's `Style.layout`.

pub mod cursor;
pub mod input;
pub mod rotary;

use crate::types::{Fixed, Rect};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorFeedbackMode {
    #[default]
    Dot,
    MagneticRect,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CursorVisual {
    pub x: Fixed,
    pub y: Fixed,
    pub down: bool,
    pub target: Option<crate::ecs::Entity>,
    pub target_rect: Option<Rect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorFeedback {
    pub enabled: bool,
    pub mode: CursorFeedbackMode,
    pub current: CursorVisual,
    pub last_event_seq: u32,
    pub(crate) entity: Option<crate::ecs::Entity>,
}

impl Default for CursorFeedback {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: CursorFeedbackMode::Dot,
            current: CursorVisual::default(),
            last_event_seq: 0,
            entity: None,
        }
    }
}

/// `entity` is monotonic: `None` until plugin's first `pre_render` spawns
/// it, then a fixed `Some(_)` for the lifetime of the app. Equality compares
/// it like the other fields, which is fine because it doesn't change after
/// spawn — if that ever stops being true, the dirty short-circuit in
/// `rotary_feedback_system` needs revisiting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RotaryFeedback {
    pub enabled: bool,
    pub progress: Fixed,
    pub target: Fixed,
    pub velocity: Fixed,
    pub direction: i8,
    pub opacity: Fixed,
    pub last_input_ms: u32,
    pub pulse: Fixed,
    pub last_input_seq: u32,
    pub(crate) entity: Option<crate::ecs::Entity>,
}

impl Default for RotaryFeedback {
    fn default() -> Self {
        Self {
            enabled: false,
            progress: Fixed::ZERO,
            target: Fixed::ZERO,
            velocity: Fixed::ZERO,
            direction: 0,
            opacity: Fixed::ZERO,
            last_input_ms: 0,
            pulse: Fixed::ZERO,
            last_input_seq: 0,
            entity: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InputFeedback {
    pub cursor: CursorFeedback,
    pub rotary: RotaryFeedback,
}

impl InputFeedback {
    pub fn enabled() -> Self {
        Self {
            cursor: CursorFeedback {
                enabled: true,
                ..CursorFeedback::default()
            },
            rotary: RotaryFeedback {
                enabled: true,
                ..RotaryFeedback::default()
            },
        }
    }
}

/// Accumulates input events between system runs. Cleared each time the
/// rotary system consumes them.
#[derive(Clone, Copy, Debug, Default)]
pub struct InputFeedbackInput {
    pub rotary_delta: i16,
    pub wheel_delta_y: Fixed,
    pub click_pulse: bool,
    pub event_seq: u32,
}

/// Marker component on the cursor overlay entity.
pub struct OverlayCursor;

impl crate::ecs::Component for OverlayCursor {}

/// Marker component on the rotary overlay entity.
pub struct OverlayRotary;

impl crate::ecs::Component for OverlayRotary {}

/// Update an overlay entity's absolute layout to track a logical-pixel rect.
/// The flex pass picks this up next frame and writes the corresponding
/// `ComputedRect`, which is what the dirty walker and view renderer read.
pub(crate) fn write_overlay_layout(
    world: &mut crate::ecs::World,
    entity: crate::ecs::Entity,
    rect: Rect,
) {
    if let Some(style) = world.get_mut::<crate::ui::Style>(entity) {
        style.layout.left = crate::types::Dimension::Px(rect.x);
        style.layout.top = crate::types::Dimension::Px(rect.y);
        style.layout.width = crate::types::Dimension::Px(rect.w);
        style.layout.height = crate::types::Dimension::Px(rect.h);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::World;
    use crate::render::renderer::{DrawRequest, RenderError, Renderer};
    use crate::test_support::tracked_allocations;
    use crate::types::Transform;
    use crate::ui::Style;
    use crate::ui::dirty::VisualDirty;
    use crate::ui::theme::WidgetState;
    use crate::ui::view::{PaintCtx, View, ViewInvalidation, ViewRegistry};
    use core::any::TypeId;

    #[derive(Default)]
    struct CountingRenderer(usize);

    impl Renderer for CountingRenderer {
        fn submit(&mut self, _: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
            self.0 += 1;
            Ok(())
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn overlay_descriptors_keep_filters_and_invocation_time_feedback() {
        for (factory, component, name, priority) in [
            (
                cursor::view as fn() -> View,
                TypeId::of::<OverlayCursor>(),
                "input_feedback_cursor",
                90,
            ),
            (
                rotary::view,
                TypeId::of::<OverlayRotary>(),
                "input_feedback_rotary",
                91,
            ),
        ] {
            let mut descriptor = None;
            assert_eq!(tracked_allocations(|| descriptor = Some(factory())), 0);
            let descriptor = descriptor.unwrap();
            assert_eq!(descriptor.name(), name);
            assert_eq!(descriptor.component_filter(), Some(component));
            assert_eq!(descriptor.priority(), Some(priority));
            assert_eq!(descriptor.invalidation(), ViewInvalidation::Visual);
            assert!(descriptor.observation().is_none());
            let render = descriptor.render().unwrap();
            let mut world = World::new();
            let entity = world.spawn_empty();
            world.insert(entity, OverlayCursor);
            world.insert(entity, OverlayRotary);
            let style = Style::default();
            let rect = Rect::new(0, 0, 36, 128);
            let mut feedback = InputFeedback::enabled();
            feedback.rotary.opacity = Fixed::ONE;
            for (value, active) in [
                (None, false),
                (Some(feedback), true),
                (Some(InputFeedback::default()), false),
                (Some(feedback), true),
                (None, false),
            ] {
                world.remove_resource::<InputFeedback>();
                if let Some(value) = value {
                    world.insert_resource(value);
                }
                let mut renderer = CountingRenderer::default();
                {
                    let mut paint = PaintCtx::new(
                        &mut renderer,
                        &style,
                        &rect,
                        &rect,
                        Transform::IDENTITY,
                        None,
                        WidgetState::Enabled,
                    );
                    let allocations = tracked_allocations(|| render(&world, entity, &mut paint));
                    assert_eq!(paint.background_handled(), active);
                    assert_eq!(paint.error(), None);
                    if !active || component == TypeId::of::<OverlayCursor>() {
                        assert_eq!(allocations, 0);
                    }
                }
                assert_eq!(renderer.0, usize::from(active));
            }
        }
    }

    #[test]
    fn overlay_marker_changes_use_descriptor_visual_invalidation() {
        fn check<T: crate::ecs::Component>(
            world: &mut World,
            entity: crate::ecs::Entity,
            make: impl Fn() -> T,
        ) {
            for _ in 0..2 {
                for _ in 0..2 {
                    world.insert(entity, make());
                    ViewRegistry::reconcile_observations(world);
                    assert!(world.get::<VisualDirty>(entity).is_some());
                    world.remove::<VisualDirty>(entity);
                }
                world.remove::<T>(entity);
                ViewRegistry::reconcile_observations(world);
                assert!(world.get::<VisualDirty>(entity).is_some());
                world.remove::<VisualDirty>(entity);
            }
        }

        let mut world = World::new();
        let mut registry = ViewRegistry::default();
        registry.insert(cursor::view());
        registry.insert(rotary::view());
        world.insert_resource(registry);
        ViewRegistry::reconcile_observations(&mut world);
        let entity = world.spawn_empty();
        check(&mut world, entity, || OverlayCursor);
        check(&mut world, entity, || OverlayRotary);
    }
}
