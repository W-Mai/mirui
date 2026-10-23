use super::*;
use crate::render::PreparedDraw;
use crate::types::Color;

#[derive(Default)]
struct DeferredRenderer {
    ready: bool,
    fail: bool,
    submissions: usize,
    clip_depth: usize,
    waiter: Option<Waker>,
}

impl Renderer for DeferredRenderer {
    type TargetState = ();

    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        Ok(RenderRoute::Native)
    }

    fn prepare<'data>(
        &mut self,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut Context<'_>,
    ) -> Poll<Result<PreparedDraw<'data>, RenderError>> {
        if matches!(request.command, DrawCommand::Fill { color, .. } if color.r == 255)
            && !self.ready
        {
            self.waiter = Some(cx.waker().clone());
            return Poll::Pending;
        }
        Poll::Ready(PreparedDraw::native(request, route, self.output_scale()))
    }

    fn submit(&mut self, draw: &PreparedDraw<'_>) -> Result<(), RenderError> {
        self.submissions += 1;
        match draw.request().command {
            DrawCommand::PushClip { .. } => self.clip_depth += 1,
            DrawCommand::PopClip => self.clip_depth -= 1,
            _ => {}
        }
        if self.fail {
            Err(RenderError::BackendFailure)
        } else {
            Ok(())
        }
    }

    fn flush(&mut self) -> Result<(), RenderError> {
        Ok(())
    }
}

fn rect(color: Color) -> DrawCommand<'static> {
    DrawCommand::Fill {
        area: Rect::new(0, 0, 4, 4),
        transform: Transform::IDENTITY,
        quad: None,
        color,
        radius: crate::types::Fixed::ZERO,
        opa: 255,
    }
}

#[test]
fn pending_does_not_submit_and_fallback_does_not_discard_the_wait() {
    let mut renderer = DeferredRenderer::default();
    let style = Style::default();
    let bounds = Rect::new(0, 0, 4, 4);
    let wake = crate::core::resource::wake::TaskWake::new();
    wake.take_ready();
    let mut paint = PaintCtx::new(
        &mut renderer,
        &style,
        &bounds,
        &bounds,
        Transform::IDENTITY,
        None,
        WidgetState::Enabled,
    );
    paint.set_waker(wake.waker());
    assert!(paint.poll_draw(&rect(Color::rgb(255, 0, 0))).is_pending());
    assert_eq!(paint.error(), None);
    assert_eq!(
        paint.poll_draw(&rect(Color::rgb(0, 255, 0))),
        Poll::Ready(Ok(()))
    );
    assert!(paint.resource_pending());
    assert_eq!(paint.error(), None);
    drop(paint);
    assert_eq!(renderer.submissions, 1);
    renderer.waiter.take().unwrap().wake();
    assert!(wake.take_ready());
}

#[test]
fn strict_pending_and_real_errors_preserve_the_first_failure() {
    let style = Style::default();
    let bounds = Rect::new(0, 0, 4, 4);
    for (ready, failure) in [
        (false, RenderError::ResourcePending),
        (true, RenderError::BackendFailure),
    ] {
        let mut renderer = DeferredRenderer {
            ready,
            fail: true,
            ..Default::default()
        };
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &bounds,
            &bounds,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        paint.draw(&rect(Color::rgb(255, 0, 0)));
        assert_eq!(paint.error(), Some(failure));
        assert_eq!(
            paint.poll_draw(&rect(Color::rgb(0, 255, 0))),
            Poll::Ready(Err(failure))
        );
        drop(paint);
        assert_eq!(renderer.submissions, usize::from(ready));
    }
}

#[test]
fn scene_replay_workspace_variants_preserve_the_paint_waiter() {
    use crate::render::scene::{SceneOp, replay::*, resolver::SliceResolver};
    let ops = [SceneOp::FillRect {
        area: Rect::new(0, 0, 4, 4),
        transform: Transform::IDENTITY,
        quad: None,
        color: Color::rgb(255, 0, 0),
        radius: crate::types::Fixed::ZERO,
        opa: 255,
    }];
    let style = Style::default();
    let bounds = Rect::new(0, 0, 4, 4);
    let resolver = SliceResolver::new(&[], &[]);
    for workspace in [false, true] {
        let wake = crate::core::resource::wake::TaskWake::new();
        wake.take_ready();
        let mut renderer = DeferredRenderer::default();
        {
            let mut paint = PaintCtx::new(
                &mut renderer,
                &style,
                &bounds,
                &bounds,
                Transform::IDENTITY,
                None,
                WidgetState::Enabled,
            );
            paint.set_waker(wake.waker());
            if workspace {
                let mut frames = [ReplayFrame::EMPTY; 2];
                paint.replay_with_workspace(&ops, &resolver, &mut frames);
            } else {
                paint.replay(&ops, &resolver);
            }
            assert_eq!(paint.error(), Some(RenderError::ResourcePending));
            assert!(paint.resource_pending());
        }
        assert_eq!(renderer.submissions, 0);
        renderer.waiter.take().unwrap().wake();
        assert!(wake.take_ready());
    }
}

#[test]
fn pending_scene_unwinds_only_its_own_clip_scopes() {
    use crate::render::scene::{SceneOp, resolver::SliceResolver};
    let ops = [
        SceneOp::PushClip {
            path: crate::render::path::Path::new(),
            transform: Transform::IDENTITY,
            fill_rule: crate::render::raster::FillRule::EvenOdd,
        },
        SceneOp::FillRect {
            area: Rect::new(0, 0, 4, 4),
            transform: Transform::IDENTITY,
            quad: None,
            color: Color::rgb(255, 0, 0),
            radius: crate::types::Fixed::ZERO,
            opa: 255,
        },
        SceneOp::PopClip,
    ];
    let mut renderer = DeferredRenderer {
        clip_depth: 1,
        ..Default::default()
    };
    let style = Style::default();
    let bounds = Rect::new(0, 0, 4, 4);
    {
        let mut paint = PaintCtx::new(
            &mut renderer,
            &style,
            &bounds,
            &bounds,
            Transform::IDENTITY,
            None,
            WidgetState::Enabled,
        );
        paint.replay(&ops, &SliceResolver::new(&[], &[]));
        assert_eq!(paint.error(), Some(RenderError::ResourcePending));
    }
    assert_eq!(renderer.clip_depth, 1);
    assert_eq!(renderer.submissions, 2);
}
