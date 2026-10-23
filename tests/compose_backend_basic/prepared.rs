use super::*;
use core::task::{Context, Poll, Waker};
use mirui::render::{DrawCommand, PreparedDraw, TextRunIdentity};

#[derive(Default)]
struct LeaseEngine {
    prepare: usize,
    begin: usize,
    end: usize,
    submit: usize,
    pending: bool,
    reject: bool,
    source: Option<*const u8>,
    clip: Option<Rect>,
    text: Option<TextRunIdentity>,
}

impl<T: Renderer> RenderEngine<T> for LeaseEngine {
    fn prepare<'data>(
        &mut self,
        target: &mut T,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut Context<'_>,
    ) -> Poll<Result<PreparedDraw<'data>, RenderError>> {
        self.prepare += 1;
        if self.pending {
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        target.prepare(request, route, cx)
    }
    fn begin(&mut self, _: &mut T) -> Result<(), RenderError> {
        self.begin += 1;
        Ok(())
    }
    fn end(&mut self, _: &mut T) {
        self.end += 1;
    }
    fn submit(&mut self, target: &mut T, draw: &PreparedDraw<'_>) -> Result<(), RenderError> {
        self.submit += 1;
        self.clip = Some(draw.request().clip);
        self.source = draw
            .texture_region()
            .and_then(|region| region.pixels())
            .map(|pixels| pixels.bytes().as_ptr());
        if self.reject {
            return Err(RenderError::BackendFailure);
        }
        target.submit(draw)
    }
    fn submit_text_run(
        &mut self,
        target: &mut T,
        draw: &PreparedDraw<'_>,
        identity: TextRunIdentity,
    ) -> Result<(), RenderError> {
        self.text = Some(identity);
        target.submit_text_run(draw, identity)
    }
}

compose_backend! {
    struct PreparedInner { target: Dummy, images: LeaseEngine }
    route { default => target, blit => images, draw_glyph_run => images }
}
compose_backend! {
    struct PreparedOuter { target: PreparedInner, images: LeaseEngine }
    route { default => target, blit => images, draw_glyph_run => images }
}

#[test]
fn nested_preparation_pins_once_and_only_begins_ready_submissions() {
    let bytes = [255; 64];
    let texture = Texture::from_ref(&bytes, 4, 4, ColorFormat::RGBA8888).unwrap();
    let mut renderer = PreparedOuter::new(
        PreparedInner::new(
            Dummy::new(),
            LeaseEngine {
                pending: true,
                ..Default::default()
            },
        ),
        LeaseEngine::default(),
    );
    let cmd = DrawCommand::Blit {
        texture: &texture,
        pos: Point::ZERO,
        size: Point::new(4, 4),
        transform: Transform::IDENTITY,
        quad: None,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    let request = DrawRequest::new(&cmd, Rect::new(1, 1, 2, 2));
    let mut cx = Context::from_waker(Waker::noop());
    let dynamic: &mut dyn Renderer = &mut renderer;
    assert!(
        dynamic
            .poll_draw_request(request, None, &mut cx)
            .is_pending()
    );
    assert_eq!(
        (renderer.images.begin, renderer.target.images.begin),
        (0, 0)
    );
    assert_eq!(renderer.target.target.counts.blit.get(), 0);
    renderer.target.images.pending = false;
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(
        tracking_allocator::tracked_allocations(|| {
            let dynamic: &mut dyn Renderer = &mut renderer;
            let route = dynamic.route(&request).unwrap();
            let Poll::Ready(Ok(draw)) = dynamic.prepare(request, route, &mut cx) else {
                panic!("ready source");
            };
            dynamic.submit(&draw).unwrap();
        }),
        0
    );
    assert_eq!(renderer.images.source, Some(bytes.as_ptr()));
    assert_eq!(renderer.target.images.source, renderer.images.source);
    assert_eq!(
        (renderer.images.prepare, renderer.target.images.prepare),
        (2, 2)
    );
    assert_eq!((renderer.images.begin, renderer.images.end), (1, 1));
    renderer.target.images.reject = true;
    assert_eq!(
        renderer.draw_request(&request),
        Err(RenderError::BackendFailure)
    );
    assert_eq!((renderer.images.begin, renderer.images.end), (2, 2));
    assert_eq!(
        (renderer.target.images.begin, renderer.target.images.end),
        (2, 2)
    );
    assert_eq!(renderer.target.target.counts.blit.get(), 1);
}

#[test]
fn nested_text_routes_preserve_run_identity() {
    let mut renderer = PreparedOuter::new(
        PreparedInner::new(Dummy::new(), LeaseEngine::default()),
        LeaseEngine::default(),
    );
    let font = mirui::render::font::Font::default();
    let command = DrawCommand::GlyphRun {
        pos: Point::ZERO,
        glyphs: &[],
        font: &font,
        transform: Transform::IDENTITY,
        color: Color::rgb(1, 2, 3),
        opa: 255,
    };
    let identity = TextRunIdentity {
        entity: mirui::ecs::Entity {
            id: 7,
            generation: 3,
        },
        run_index: 2,
    };
    let mut cx = Context::from_waker(Waker::noop());
    let dynamic: &mut dyn Renderer = &mut renderer;
    assert_eq!(
        dynamic.poll_draw_request(
            DrawRequest::new(&command, Rect::new(0, 0, 4, 4)),
            Some(identity),
            &mut cx,
        ),
        Poll::Ready(Ok(()))
    );
    assert_eq!(renderer.images.text, Some(identity));
    assert_eq!(renderer.target.images.text, Some(identity));
    assert_eq!(
        (renderer.images.prepare, renderer.target.images.prepare),
        (1, 1)
    );
    assert_eq!((renderer.images.begin, renderer.images.end), (1, 1));
}

#[test]
fn nested_routes_keep_each_prepared_request_independent() {
    let bytes = [255; 256];
    let texture = Texture::from_ref(&bytes, 8, 8, ColorFormat::RGBA8888).unwrap();
    let mut renderer = PreparedOuter::new(
        PreparedInner::new(Dummy::new(), LeaseEngine::default()),
        LeaseEngine::default(),
    );
    let command = DrawCommand::Blit {
        texture: &texture,
        pos: Point::ZERO,
        size: Point::new(8, 8),
        transform: Transform::IDENTITY,
        quad: None,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(
        tracking_allocator::tracked_allocations(|| {
            let a = DrawRequest::new(&command, Rect::new(1, 1, 2, 2));
            let b = DrawRequest::new(&command, Rect::new(4, 4, 2, 2));
            let Poll::Ready(Ok(a)) = renderer.prepare(a, RenderRoute::Native, &mut cx) else {
                panic!("first region");
            };
            let Poll::Ready(Ok(b)) = renderer.prepare(b, RenderRoute::Native, &mut cx) else {
                panic!("second region");
            };
            renderer.submit(&b).unwrap();
            assert_eq!(renderer.images.source, Some(bytes.as_ptr()));
            assert_eq!(renderer.images.clip, Some(Rect::new(4, 4, 2, 2)));
            assert_eq!(renderer.target.images.clip, renderer.images.clip);
            renderer.submit(&a).unwrap();
            assert_eq!(renderer.images.source, Some(bytes.as_ptr()));
            assert_eq!(renderer.images.clip, Some(Rect::new(1, 1, 2, 2)));
            assert_eq!(renderer.target.images.clip, renderer.images.clip);
            assert_eq!(renderer.target.images.source, renderer.images.source);
        }),
        0
    );
    assert_eq!((renderer.images.begin, renderer.images.end), (2, 2));
    assert_eq!(
        (renderer.target.images.begin, renderer.target.images.end),
        (2, 2)
    );
}

#[derive(Default)]
struct UploadBackend {
    complete: bool,
    submitted: Option<mirui::render::texture::Region>,
}

impl Renderer for UploadBackend {
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
        if self.complete {
            PreparedDraw::poll_complete(request, route, self.output_scale(), cx)
        } else {
            PreparedDraw::poll_upload(request, route, self.output_scale(), cx)
        }
    }

    fn submit(&mut self, draw: &PreparedDraw<'_>) -> Result<(), RenderError> {
        let source = draw.texture_region().unwrap();
        self.submitted = Some(source.region());
        assert_eq!(draw.request().clip, Rect::new(1, 1, 2, 2));
        let DrawCommand::Blit { size, .. } = *draw.request().command else {
            panic!("texture");
        };
        assert_eq!(size, Point::new(8, 8));
        for (area, pixels) in source.tiles() {
            assert_eq!(area, source.region());
            assert_eq!(
                (u32::from(pixels.width()), u32::from(pixels.height())),
                (area.width(), area.height())
            );
            assert!(pixels.bytes().iter().all(|byte| *byte == 255));
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), RenderError> {
        Ok(())
    }
}

impl<T: Renderer> RenderEngine<T> for UploadBackend {
    fn prepare<'data>(
        &mut self,
        _: &mut T,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut Context<'_>,
    ) -> Poll<Result<PreparedDraw<'data>, RenderError>> {
        Renderer::prepare(self, request, route, cx)
    }

    fn submit(&mut self, _: &mut T, draw: &PreparedDraw<'_>) -> Result<(), RenderError> {
        Renderer::submit(self, draw)
    }
}

#[test]
fn third_party_upload_preparation_is_available_through_nested_routes() {
    use mirui::render::texture::Region;
    static BYTES: [u8; 256] = [255; 256];
    let texture = Texture::from_static(&BYTES, 8, 8, ColorFormat::RGBA8888).unwrap();
    let command = DrawCommand::Blit {
        texture: &texture,
        pos: Point::ZERO,
        size: Point::new(8, 8),
        transform: Transform::IDENTITY,
        quad: None,
        opa: 255,
        radius: Fixed::ZERO,
        composite: CompositeMode::SourceOver,
    };
    for complete in [false, true] {
        let mut renderer = PreparedOuter::new(
            PreparedInner::new(
                Dummy::new(),
                UploadBackend {
                    complete,
                    ..Default::default()
                },
            ),
            LeaseEngine::default(),
        );
        assert_eq!(
            tracking_allocator::tracked_allocations(|| {
                let dynamic: &mut dyn Renderer = &mut renderer;
                dynamic
                    .draw_request(&DrawRequest::new(&command, Rect::new(1, 1, 2, 2)))
                    .unwrap();
            }),
            0
        );
        assert_eq!(
            renderer.target.images.submitted,
            Some(
                Region::new(
                    0,
                    0,
                    if complete { 8 } else { 4 },
                    if complete { 8 } else { 4 }
                )
                .unwrap()
            )
        );
        assert_eq!(renderer.images.source, Some(BYTES.as_ptr()));
        assert_eq!((renderer.images.begin, renderer.images.end), (1, 1));
    }
}
