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

#[test]
fn nested_coordinate_wrappers_forward_one_request_and_preserve_pending_context() {
    use crate::types::{Color, Point, Transform};
    use crate::ui::render_system::ScopedRenderer;

    struct Leaf {
        calls: usize,
        projection: Transform3D,
        transform: Transform,
        quad: Option<[Point; 4]>,
        physical: Option<PhysicalRect>,
        identity: TextRunIdentity,
        waker: Waker,
        result: Poll<Result<(), RenderError>>,
    }
    impl Renderer for Leaf {
        type TargetState = ();
        fn poll_draw_request(
            &mut self,
            request: DrawRequest<'_, '_>,
            text: Option<TextRunIdentity>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), RenderError>> {
            self.calls += 1;
            assert_eq!(request.projective, self.projection);
            assert_eq!(request.physical_clip, self.physical);
            assert_eq!(request.clip, Rect::new(-5, -7, 32, 32));
            let DrawCommand::Fill {
                transform, quad, ..
            } = request.command
            else {
                panic!("command identity changed");
            };
            assert_eq!(*transform, self.transform);
            assert_eq!(*quad, self.quad);
            assert_eq!(text, Some(self.identity));
            assert!(cx.waker().will_wake(&self.waker));
            self.result
        }
        fn route(&self, _: &DrawRequest<'_, '_>) -> Result<RenderRoute, RenderError> {
            panic!("wrappers must forward the execution entry")
        }
        fn submit(&mut self, _: &PreparedDraw<'_>) -> Result<(), RenderError> {
            panic!("wrappers must not separately submit")
        }
        fn flush(&mut self) -> Result<(), RenderError> {
            Ok(())
        }
    }

    let identity = TextRunIdentity {
        entity: Entity {
            id: 7,
            generation: 2,
        },
        run_index: 4,
    };
    let waker = Waker::from(Arc::new(WakeCount::default()));
    let mut cx = Context::from_waker(&waker);
    let translation = Transform::translate(Fixed::from_int(-5), Fixed::from_int(-7));
    let projection = Transform3D::rotate_y_perspective(Fixed::from_int(20), Fixed::from_int(400));
    let quad = [
        Point::new(8, 9),
        Point::new(12, 9),
        Point::new(12, 13),
        Point::new(8, 13),
    ];
    for (scope_transform, request_transform, explicit) in [
        (Transform3D::IDENTITY, Transform3D::IDENTITY, None),
        (projection, Transform3D::IDENTITY, None),
        (Transform3D::IDENTITY, projection, None),
        (projection, projection, None),
        (Transform3D::IDENTITY, Transform3D::IDENTITY, Some(quad)),
    ] {
        let projective = scope_transform.compose(&request_transform);
        for result in [
            Poll::Pending,
            Poll::Ready(Ok(())),
            Poll::Ready(Err(RenderError::BackendFailure)),
        ] {
            let mut leaf = Leaf {
                calls: 0,
                projection: if projective.is_identity() {
                    projective
                } else {
                    Transform3D::from_affine(translation).compose(&projective)
                },
                transform: if projective.is_identity() && explicit.is_none() {
                    translation
                } else {
                    Transform::IDENTITY
                },
                quad: explicit.map(|q| q.map(|p| translation.apply_point(p))),
                physical: Some(PhysicalRect::new(3, 3, 8, 8).unwrap()),
                identity,
                waker: waker.clone(),
                result,
            };
            let command = DrawCommand::Fill {
                area: Rect::new(8, 9, 4, 4),
                transform: Transform::IDENTITY,
                quad: explicit,
                color: Color::rgb(1, 2, 3),
                radius: Fixed::ZERO,
                opa: 255,
            };
            let mut region = RegionRenderer::new(&mut leaf, (5, 7));
            let mut scope = ScopedRenderer::new(
                &mut region,
                &scope_transform,
                Some(PhysicalRect::new(8, 10, 8, 8).unwrap()),
            );
            let request = DrawRequest::new(&command, Rect::new(0, 0, 32, 32))
                .with_projective(request_transform)
                .with_physical_clip(PhysicalRect::new(6, 8, 20, 20).unwrap());
            assert_eq!(
                scope.poll_draw_request(request, Some(identity), &mut cx),
                result
            );
            assert_eq!(leaf.calls, 1);
        }
    }
}
