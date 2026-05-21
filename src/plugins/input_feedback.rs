//! Input feedback plugin — wires the cursor and rotary overlay
//! [`crate::feedback`] systems and views into [`App`].
//!
//! Cursor entity is lazily spawned by `cursor_feedback_system` on the
//! first [`PointerCursor`]. Rotary entity is spawned eagerly in this
//! plugin's `pre_render` once `WidgetRoot` is set, so its absolute
//! Style is in place before the rotary system runs.

use crate::app::{App, RendererFactory};
use crate::ecs::World;
use crate::event::input::InputEvent;
use crate::feedback::{InputFeedback, InputFeedbackInput, cursor, input as feedback_input, rotary};
use crate::plugin::Plugin;
use crate::surface::Surface;
use crate::widget::WidgetRoot;
use crate::widget::view::ViewRegistry;

pub struct InputFeedbackPlugin {
    rotary_spawned: bool,
}

impl InputFeedbackPlugin {
    pub fn new() -> Self {
        Self {
            rotary_spawned: false,
        }
    }

    /// Internal lazy spawn step. Exposed without `B/F` generics so tests
    /// can drive it without instantiating the full `Plugin<B, F>` impl.
    fn ensure_rotary_spawned(&mut self, world: &mut World) {
        if self.rotary_spawned {
            return;
        }
        if let Some(root) = world.resource::<WidgetRoot>().copied() {
            rotary::spawn_overlay_rotary(world, root.0);
            self.rotary_spawned = true;
        }
    }
}

impl Default for InputFeedbackPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl<B, F> Plugin<B, F> for InputFeedbackPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, app: &mut App<B, F>) {
        app.world.insert_resource(InputFeedback::enabled());
        app.world.insert_resource(InputFeedbackInput::default());
        app.add_system(cursor::cursor_feedback_system::system());
        app.add_system(rotary::rotary_feedback_system::system());
        if let Some(reg) = app.world.resource_mut::<ViewRegistry>() {
            reg.insert(cursor::view());
            reg.insert(rotary::view());
        }
    }

    fn on_event(&mut self, world: &mut World, event: &InputEvent) -> bool {
        feedback_input::record_input(world, event);
        false
    }

    fn pre_render(&mut self, world: &mut World) {
        self.ensure_rotary_spawned(world);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::Entity;
    use crate::feedback::{OverlayCursor, OverlayRotary};
    use crate::widget::view::ViewRegistry;
    use crate::widget::{Children, WidgetRoot};

    fn make_app() -> App<crate::surface::framebuf::FramebufSurface<fn(&[u8], &crate::types::Rect)>>
    {
        crate::app::App::headless(64, 64).with_default_widgets()
    }

    fn child_with<C: 'static>(world: &World, root: Entity) -> Option<Entity> {
        let children = world.get::<Children>(root)?;
        children
            .0
            .iter()
            .copied()
            .find(|e| world.get::<C>(*e).is_some())
    }

    #[test]
    fn build_inserts_resources_and_views() {
        let mut app = make_app();
        app.add_plugin(InputFeedbackPlugin::new());

        assert!(app.world.resource::<InputFeedback>().is_some());
        assert!(app.world.resource::<InputFeedbackInput>().is_some());
        let reg = app.world.resource::<ViewRegistry>().unwrap();
        let names: alloc::vec::Vec<&str> = reg.iter().map(|v| v.name()).collect();
        assert!(names.contains(&"input_feedback_cursor"));
        assert!(names.contains(&"input_feedback_rotary"));
    }

    #[test]
    fn overlay_views_have_priority_above_builtin_widgets() {
        let mut app = make_app();
        app.add_plugin(InputFeedbackPlugin::new());
        let reg = app.world.resource::<ViewRegistry>().unwrap();
        // Built-in widgets cap at priority 80 (Text). Overlays must come last.
        let names: alloc::vec::Vec<&str> = reg.iter().map(|v| v.name()).collect();
        let cursor_idx = names
            .iter()
            .position(|n| *n == "input_feedback_cursor")
            .unwrap();
        let rotary_idx = names
            .iter()
            .position(|n| *n == "input_feedback_rotary")
            .unwrap();
        let text_idx = names.iter().position(|n| *n == "Text").unwrap();
        assert!(cursor_idx > text_idx);
        assert!(rotary_idx > text_idx);
    }

    #[test]
    fn pre_render_lazy_spawns_rotary_after_root_set() {
        let mut app = make_app();
        let root = app.world.spawn();
        app.world.insert(root, crate::widget::Widget);
        app.world.insert(root, crate::widget::Style::default());
        app.world.insert_resource(WidgetRoot(root));
        app.add_plugin(InputFeedbackPlugin::new());
        // build() runs immediately on add_plugin, but plugin's pre_render hasn't
        // fired yet because no render frame happened.
        assert!(child_with::<OverlayRotary>(&app.world, root).is_none());

        let mut plugin = InputFeedbackPlugin::new();
        plugin.ensure_rotary_spawned(&mut app.world);

        assert!(child_with::<OverlayRotary>(&app.world, root).is_some());
    }

    /// Architecture invariant: the framework `event::dispatch_input` path
    /// must not reach into the feedback module. The source is `include_str!`
    /// so the check works in `no_std` test runs too.
    #[test]
    fn event_dispatch_does_not_depend_on_feedback() {
        let src = include_str!("../event/mod.rs");
        assert!(
            !src.contains("input_feedback") && !src.contains("feedback::"),
            "event/mod.rs must not depend on feedback module",
        );
    }

    #[test]
    fn cursor_overlay_not_eagerly_spawned() {
        // Cursor entity must wait for first PointerCursor; otherwise the
        // ESP rotary-only path leaks an unused entity into every frame.
        let mut app = make_app();
        let root = app.world.spawn();
        app.world.insert(root, crate::widget::Widget);
        app.world.insert(root, crate::widget::Style::default());
        app.world.insert_resource(WidgetRoot(root));
        app.add_plugin(InputFeedbackPlugin::new());
        let mut plugin = InputFeedbackPlugin::new();
        plugin.ensure_rotary_spawned(&mut app.world);

        assert!(child_with::<OverlayCursor>(&app.world, root).is_none());
    }
}
