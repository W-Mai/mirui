use super::*;
use core::cell::RefCell;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::task::Wake;

#[derive(Default)]
struct WakeCount(AtomicUsize);

impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[derive(Default)]
struct Recorder {
    calls: RefCell<alloc::vec::Vec<u8>>,
    fail: u8,
    pending: bool,
    wake: Option<Waker>,
    text: Option<TextRunIdentity>,
}

impl Renderer for Recorder {
    type TargetState = ();

    fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
        self.calls.borrow_mut().push(1);
        if self.fail == 1 {
            return Err(RenderError::BackendFailure);
        }
        Ok(RenderRoute::Native)
    }

    fn prepare<'data>(
        &mut self,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut Context<'_>,
    ) -> Poll<Result<PreparedDraw<'data>, RenderError>> {
        self.calls.borrow_mut().push(2);
        if self.fail == 2 {
            return Poll::Ready(Err(RenderError::BackendFailure));
        }
        if self.pending {
            self.wake = Some(cx.waker().clone());
            return Poll::Pending;
        }
        PreparedDraw::poll_cpu(request, route, self.output_scale(), cx)
    }

    fn submit(&mut self, _: &PreparedDraw<'_>) -> Result<(), RenderError> {
        self.calls.borrow_mut().push(3);
        if self.fail == 3 {
            return Err(RenderError::BackendFailure);
        }
        Ok(())
    }

    fn submit_text_run(
        &mut self,
        draw: &PreparedDraw<'_>,
        identity: TextRunIdentity,
    ) -> Result<(), RenderError> {
        self.text = Some(identity);
        self.submit(draw)
    }

    fn flush(&mut self) -> Result<(), RenderError> {
        Ok(())
    }
}

#[test]
fn dispatch_short_circuits_errors_and_pending_without_losing_the_waker() {
    let command = DrawCommand::PopClip;
    let request = DrawRequest::new(&command, Rect::new(0, 0, 4, 4));
    let count = Arc::new(WakeCount::default());
    let waker = Waker::from(count.clone());
    let mut cx = Context::from_waker(&waker);
    for fail in 1..=3 {
        let mut recorder = Recorder {
            fail,
            ..Default::default()
        };
        assert_eq!(
            recorder.poll_draw_request(request, None, &mut cx),
            Poll::Ready(Err(RenderError::BackendFailure))
        );
        assert_eq!(
            *recorder.calls.borrow(),
            (1..=fail).collect::<alloc::vec::Vec<_>>()
        );
    }
    let mut recorder = Recorder {
        pending: true,
        ..Default::default()
    };
    assert!(
        recorder
            .poll_draw_request(request, None, &mut cx)
            .is_pending()
    );
    assert_eq!(*recorder.calls.borrow(), [1, 2]);
    recorder.wake.take().unwrap().wake();
    assert_eq!(count.0.load(Ordering::Relaxed), 1);
    recorder.pending = false;
    recorder.calls.borrow_mut().clear();
    assert_eq!(
        recorder.poll_draw_request(request, None, &mut cx),
        Poll::Ready(Ok(()))
    );
    assert_eq!(*recorder.calls.borrow(), [1, 2, 3]);
}

#[test]
fn immediate_helpers_preserve_text_identity_and_pending_semantics() {
    let command = DrawCommand::PopClip;
    let request = DrawRequest::new(&command, Rect::new(0, 0, 4, 4));
    let identity = TextRunIdentity {
        entity: Entity {
            id: 7,
            generation: 2,
        },
        run_index: 3,
    };
    let mut recorder = Recorder::default();
    recorder.draw_text_run(&request, identity).unwrap();
    assert_eq!(recorder.text, Some(identity));
    assert_eq!(*recorder.calls.borrow(), [1, 2, 3]);
    recorder.pending = true;
    for text in [None, Some(identity)] {
        recorder.calls.borrow_mut().clear();
        let result = match text {
            Some(identity) => recorder.draw_text_run(&request, identity),
            None => recorder.draw_request(&request),
        };
        assert_eq!(result, Err(RenderError::ResourcePending));
        assert_eq!(*recorder.calls.borrow(), [1, 2]);
    }
}
