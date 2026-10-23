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
    assert_eq!(
        renderer.draw_request(&request),
        Err(RenderError::ResourcePending)
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
    assert_eq!(renderer.images.source, Some(bytes[20..].as_ptr()));
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
    renderer
        .draw_text_run(&DrawRequest::new(&command, Rect::new(0, 0, 4, 4)), identity)
        .unwrap();
    assert_eq!(renderer.images.text, Some(identity));
    assert_eq!(renderer.target.images.text, Some(identity));
    assert_eq!(
        (renderer.images.prepare, renderer.target.images.prepare),
        (1, 1)
    );
    assert_eq!((renderer.images.begin, renderer.images.end), (1, 1));
}
