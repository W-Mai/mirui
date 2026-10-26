use super::*;
use core::task::{Context, Poll, Waker};
use mirui::render::{DrawCommand, PreparedDraw};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::task::Wake;

#[derive(Default)]
struct Pipeline {
    calls: Cell<[usize; 5]>,
    fail: usize,
    pending: bool,
    rewrite: bool,
}

impl Pipeline {
    fn record(&self, stage: usize) -> Result<(), RenderError> {
        let mut calls = self.calls.get();
        calls[stage - 1] += 1;
        self.calls.set(calls);
        if self.fail == stage {
            Err(RenderError::BackendFailure)
        } else {
            Ok(())
        }
    }
}

impl<T: Renderer> RenderEngine<T> for Pipeline {
    fn route(&self, target: &T, request: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        self.record(1)?;
        target.route(request)
    }

    fn prepare<'data>(
        &mut self,
        target: &mut T,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut Context<'_>,
    ) -> Poll<Result<PreparedDraw<'data>, RenderError>> {
        self.record(2)?;
        if self.pending {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if self.rewrite {
            let command = DrawCommand::Fill {
                area: request.clip,
                transform: Transform::IDENTITY,
                quad: None,
                color: Color::rgb(10, 20, 30),
                radius: Fixed::ZERO,
                opa: 255,
            };
            PreparedDraw::poll_cpu(
                DrawRequest {
                    command: &command,
                    ..request
                },
                route,
                target.output_scale(),
                cx,
            )
        } else {
            target.prepare(request, route, cx)
        }
    }

    fn begin(&mut self, _: &mut T) -> Result<(), RenderError> {
        self.record(3)
    }

    fn submit(&mut self, target: &mut T, draw: &PreparedDraw<'_>) -> Result<(), RenderError> {
        self.record(4)?;
        target.submit(draw)
    }

    fn end(&mut self, _: &mut T) {
        self.record(5).unwrap();
    }
}

compose_backend! {
    struct ImagePipeline { target: Dummy, images: Pipeline }
    route { default => target, blit => images }
}

compose_backend! {
    struct FillPipeline { target: ImagePipeline, fills: Pipeline }
    route { default => target, fill_rect => fills }
}

#[derive(Default)]
struct WakeCount(AtomicUsize);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn blit<'a>(texture: &'a Texture<'_>) -> DrawCommand<'a> {
    DrawCommand::Blit {
        texture,
        pos: Point::ZERO,
        size: Point::new(4, 4),
        transform: Transform::IDENTITY,
        quad: None,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    }
}

#[test]
fn composed_dispatch_preserves_failure_boundaries_and_pending_wakes() {
    let source = Texture::from_static(&[255; 64], 4, 4, ColorFormat::RGBA8888).unwrap();
    let command = blit(&source);
    let request = DrawRequest::new(&command, Rect::new(0, 0, 4, 4));
    let wake = Arc::new(WakeCount::default());
    let waker = Waker::from(wake.clone());
    let mut cx = Context::from_waker(&waker);
    let expected = [
        [1, 1, 1, 1, 1],
        [1, 0, 0, 0, 0],
        [1, 1, 0, 0, 0],
        [1, 1, 1, 0, 0],
        [1, 1, 1, 1, 1],
    ];
    for (fail, calls) in expected.into_iter().enumerate() {
        let mut renderer = ImagePipeline::new(
            Dummy::new(),
            Pipeline {
                fail,
                ..Default::default()
            },
        );
        assert_eq!(
            tracking_allocator::tracked_allocations(|| {
                let dynamic: &mut dyn Renderer = &mut renderer;
                assert_eq!(
                    dynamic.poll_draw_request(request, None, &mut cx),
                    Poll::Ready(if fail == 0 {
                        Ok(())
                    } else {
                        Err(RenderError::BackendFailure)
                    })
                );
            }),
            0
        );
        assert_eq!(renderer.images.calls.get(), calls);
        assert_eq!(renderer.target.counts.blit.get(), u32::from(fail == 0));
    }
    let mut renderer = ImagePipeline::new(
        Dummy::new(),
        Pipeline {
            pending: true,
            ..Default::default()
        },
    );
    assert_eq!(
        renderer.poll_draw_request(request, None, &mut cx),
        Poll::Pending
    );
    assert_eq!(renderer.images.calls.get(), [1, 1, 0, 0, 0]);
    assert_eq!(wake.0.load(Ordering::Relaxed), 1);
    renderer.images.pending = false;
    assert_eq!(
        renderer.poll_draw_request(request, None, &mut cx),
        Poll::Ready(Ok(()))
    );
    assert_eq!(renderer.images.calls.get(), [2, 2, 1, 1, 1]);
}

#[test]
fn preparation_can_change_the_command_across_nested_default_routes() {
    let source = Texture::from_static(&[255; 64], 4, 4, ColorFormat::RGBA8888).unwrap();
    let command = blit(&source);
    let mut renderer = FillPipeline::new(
        ImagePipeline::new(
            Dummy::new(),
            Pipeline {
                rewrite: true,
                ..Default::default()
            },
        ),
        Pipeline::default(),
    );
    renderer
        .draw_request(&DrawRequest::new(&command, Rect::new(0, 0, 4, 4)))
        .unwrap();
    assert_eq!(renderer.target.images.calls.get(), [1, 1, 0, 0, 0]);
    assert_eq!(renderer.fills.calls.get(), [0, 0, 1, 1, 1]);
    assert_eq!(renderer.target.target.counts.blit.get(), 0);
}
