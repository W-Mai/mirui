use super::render::clip_path_render;
use super::scene::{CLIP_CIRCLE, LOGICAL_SIZE};
use crate::prelude::draw::*;
use crate::prelude::*;
use crate::render::renderer::{DrawRequest, RenderError, RenderRoute};
use crate::types::Transform;
use crate::ui::theme::WidgetState;

#[derive(Default)]
struct FailingRenderer {
    pushed: bool,
    pops: usize,
}

impl Renderer for FailingRenderer {
    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        Ok(RenderRoute::Native)
    }

    fn submit(&mut self, request: &DrawRequest<'_, '_>) -> Result<(), RenderError> {
        if self.pushed && matches!(request.command, DrawCommand::Fill { .. }) {
            return Err(RenderError::BackendFailure);
        }
        match request.command {
            DrawCommand::PushClip { .. } => self.pushed = true,
            DrawCommand::PopClip => {
                self.pushed = false;
                self.pops += 1;
            }
            _ => {}
        }
        Ok(())
    }

    fn flush(&mut self) {}
}

#[test]
fn failed_content_still_pops_clip() {
    let mut renderer = FailingRenderer::default();
    let mut world = World::new();
    let entity = world.spawn_empty();
    let style = Style::default();
    let clip = Rect::new(0, 0, 320, 320);
    let mut ctx = ViewCtx {
        style: &style,
        transform: Transform::IDENTITY,
        quad: None,
        clip: &clip,
        bg_handled: false,
        state: WidgetState::Enabled,
        error: None,
    };

    clip_path_render(&mut renderer, &world, entity, &clip, &mut ctx);

    assert_eq!(ctx.error, Some(RenderError::BackendFailure));
    assert!(!renderer.pushed);
    assert_eq!(renderer.pops, 1);
}

#[test]
fn clip_geometry_stays_in_static_storage() {
    assert!(CLIP_CIRCLE.is_borrowed());
}

#[test]
fn logical_canvas_stays_inside_phone_bounds() {
    let rect = Rect::new(8, 72, 304, 480);
    let transform =
        crate::gallery::fit_logical_canvas(rect, Transform::IDENTITY, LOGICAL_SIZE, LOGICAL_SIZE);
    let top_left = transform.apply_point(Point::ZERO);
    let bottom_right = transform.apply_point(Point::new(
        Fixed::from_int(LOGICAL_SIZE),
        Fixed::from_int(LOGICAL_SIZE),
    ));
    assert!(top_left.x >= rect.x && top_left.y >= rect.y);
    assert!(bottom_right.x <= rect.x + rect.w);
    assert!(bottom_right.y <= rect.y + rect.h);
}
