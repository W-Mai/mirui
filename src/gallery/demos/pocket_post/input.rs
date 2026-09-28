use super::state::PostSurface;
use crate::ecs::DeltaTimeMs;
use crate::gallery::play::post::{PostModel, PostModelHandle};
use crate::input::event::HandlerCtx;
use crate::input::event::gesture::GestureEvent;
use crate::prelude::plugin::Plugin;
use crate::prelude::*;
use crate::surface::InputEvent;
use crate::ui::ComputedRect;

fn local_point(rect: Rect, x: Fixed, y: Fixed) -> Option<Point> {
    if rect.w.is_zero() || rect.h.is_zero() {
        return None;
    }
    Some(Point {
        x: (x - rect.x) * Fixed::from_int(480) / rect.w,
        y: (y - rect.y) * Fixed::from_int(320) / rect.h,
    })
}

pub(super) fn surface_gesture(ctx: &HandlerCtx<'_, GestureEvent>) -> bool {
    let GestureEvent::Tap { x, y, .. } = ctx.event else {
        return false;
    };
    let Some(model) = ctx
        .component::<PostSurface>(ctx.entity)
        .map(|surface| surface.model.clone())
    else {
        return false;
    };
    let Some(rect) = ctx.component::<ComputedRect>(ctx.entity).map(|rect| rect.0) else {
        return false;
    };
    let Some(point) = local_point(rect, *x, *y) else {
        return false;
    };
    for (index, center_x) in [151, 265].into_iter().enumerate() {
        let dx = point.x - Fixed::from_int(center_x);
        let dy = point.y - Fixed::from_int(157);
        if dx * dx + dy * dy <= Fixed::from_int(23) * Fixed::from_int(23) {
            model.toggle_switch(index);
            return true;
        }
    }
    false
}

#[mirui_macros::system(order = ANIMATION, bind(model))]
pub(super) fn post_tick_system(model: &PostModel, delta: Option<DeltaTimeMs>) {
    model.advance_ms(delta.map_or(16, |delta| delta.0));
}

pub(super) struct PostKeyboardPlugin {
    model: PostModelHandle,
}

impl PostKeyboardPlugin {
    pub(super) fn new(model: PostModelHandle) -> Self {
        Self { model }
    }

    fn handle_char(&self, ch: char) -> bool {
        match ch {
            'a' | 'A' => self.model.toggle_switch(0),
            's' | 'S' => self.model.toggle_switch(1),
            ' ' => self.model.toggle_running(),
            _ => return false,
        };
        true
    }
}

impl<B, F> Plugin<B, F> for PostKeyboardPlugin
where
    B: Surface,
    F: RendererFactory<B>,
{
    fn build(&mut self, _app: &mut App<B, F>) {}

    fn on_event(&mut self, _world: &mut World, event: &InputEvent) -> bool {
        let InputEvent::CharInput { ch } = event else {
            return false;
        };
        self.handle_char(*ch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ModelHandle;
    use crate::gallery::play::post::PostModal;

    #[test]
    fn keyboard_commands_keep_using_the_registered_model_after_the_original_handle_is_dropped() {
        let mut app = App::headless(1, 1);
        let model = app.add_model(PostModel::default());
        let plugin = PostKeyboardPlugin::new(model.clone());
        drop(model);

        assert!(plugin.handle_char('a'));
        assert!(plugin.handle_char('S'));
        assert!(plugin.handle_char(' '));
        plugin.model.read(|model| {
            assert_eq!(model.switch(0), 1);
            assert_eq!(model.switch(1), 1);
            assert!(model.running());
        });

        plugin.model.open_manifests();
        assert!(plugin.handle_char('A'));
        assert!(plugin.handle_char('s'));
        assert!(plugin.handle_char(' '));
        plugin.model.read(|model| {
            assert_eq!(model.modal(), PostModal::Manifests);
            assert_eq!(model.switch(0), 1);
            assert_eq!(model.switch(1), 1);
            assert!(!model.running());
        });
        assert!(!plugin.handle_char('x'));
    }
}
