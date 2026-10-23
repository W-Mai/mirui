use super::*;
use mirui::core::cache::HasSize;
use mirui::render::command::CompositeMode;
use mirui::render::texture::Region;
use mirui::render::{
    DrawCommand, DrawRequest, PreparedDraw, RenderEngine, RenderError, RenderRoute, Renderer,
    SwRenderer,
};
use mirui::types::{Fixed, Point, Rect, Transform};
use mirui_macros::compose_backend;

static PIXELS: [u8; 16] = [
    255, 0, 0, 255, 0xa5, 0xa5, 0xa5, 0xa5, 0, 0, 255, 255, 0xa5, 0xa5, 0xa5, 0xa5,
];

struct MappedReader {
    bytes: Rc<Vec<u8>>,
    known_length: bool,
}

impl AssetReader for MappedReader {
    type Read = core::future::Ready<ReadComplete>;

    fn info(&self) -> SourceInfo {
        SourceInfo {
            len: self.known_length.then_some(self.bytes.len() as u64),
            access: SourceAccess::Random,
            consistency: SourceConsistency::Stable,
        }
    }

    fn mapped(&self) -> Option<&[u8]> {
        Some(&self.bytes)
    }

    fn read(&self, _: ReadPosition, _: AssetBuffer) -> Self::Read {
        panic!("mapped consumers must not submit physical reads")
    }
}

struct OpenMapping {
    control: Rc<Control>,
    reader: Option<MappedReader>,
}

impl Future for OpenMapping {
    type Output = Result<MappedReader, AssetError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.control.wake.replace(Some(cx.waker().clone()));
        if !self.control.ready.get() {
            return Poll::Pending;
        }
        Poll::Ready(Ok(self.reader.take().unwrap()))
    }
}

fn flat() -> Rc<Vec<u8>> {
    Rc::new(
        mirx::Document::new_flat(mirx::image::ImageAsset::new(
            1,
            2,
            mirx::image::ColorFormat::RGBA8888,
            8,
            Cow::Borrowed(&PIXELS),
        ))
        .unwrap()
        .finish()
        .unwrap()
        .into_owned(),
    )
}

fn chunk() -> Rc<Vec<u8>> {
    let mut document = mirx::Document::new();
    document
        .push_image_with_flags(
            &mirx::image::ImageAsset::new(
                1,
                1,
                mirx::image::ColorFormat::RGB565,
                2,
                Cow::Borrowed(&[0, 0]),
            ),
            mirx::ChunkFlags::CRITICAL,
        )
        .unwrap();
    let primary = document
        .push_image(&mirx::image::ImageAsset::new(
            1,
            2,
            mirx::image::ColorFormat::RGBA8888,
            8,
            Cow::Borrowed(&PIXELS),
        ))
        .unwrap();
    document.set_primary(primary).unwrap();
    Rc::new(document.finish().unwrap().into_owned())
}

fn pixel_offset(bytes: &[u8]) -> usize {
    let reader = mirx::Reader::open(bytes).unwrap();
    let surface = if let Some(image) = reader.flat_image() {
        image.surface().unwrap()
    } else {
        let mirx::image::ImageRef::Raw(surface) =
            reader.primary().unwrap().unwrap().image().unwrap().unwrap()
        else {
            panic!("raw fixture");
        };
        surface
    };
    surface.plane(0).unwrap().bytes().as_ptr() as usize - bytes.as_ptr() as usize
}

fn app(bytes: Rc<Vec<u8>>, control: Rc<Control>, known_length: bool) -> Headless {
    let mut app: Headless = App::headless(1, 2);
    app.with_widget(mirui::ui::widgets::image::view());
    app.add_plugin(
        ImageResourcesPlugin::empty().with_source("files", |_| Some("cover.mirx".into())),
    );
    app.add_plugin(ResourcePlugin::new().with_source("files", move |_| {
        control.opens.set(control.opens.get() + 1);
        OpenMapping {
            control: Rc::clone(&control),
            reader: Some(MappedReader {
                bytes: Rc::clone(&bytes),
                known_length,
            }),
        }
    }));
    app
}

#[test]
fn mapped_mirx_publishes_after_open_and_keeps_pixels_beyond_app_lifetime() {
    for bytes in [flat(), chunk()] {
        mapped_lifetime(bytes);
    }
}

fn mapped_lifetime(bytes: Rc<Vec<u8>>) {
    let weak = Rc::downgrade(&bytes);
    let control = Rc::new(Control::default());
    let mut app = app(Rc::clone(&bytes), Rc::clone(&control), true);
    let root = app.spawn_root().id();
    app.compose(root, |cx| {
        ui! { Image(src: "cover", width: 1, height: 2) }
    });
    app.tick();
    assert_eq!(control.opens.get(), 1);
    control.complete();
    for _ in 0..4 {
        app.tick();
    }
    assert_eq!(
        app.backend.framebuffer().read_pixels().unwrap().bytes(),
        &[255, 0, 0, 255, 0, 0, 255, 255]
    );
    let images = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    let texture = images.resolve("cover");
    assert_eq!((texture.width(), texture.height()), (1, 2));
    assert_eq!(images.probe("cover").unwrap().height, 2);
    assert_eq!(control.opens.get(), 1, "probe and value share the source");
    assert!(texture.cache_size() >= bytes.len());
    let original_pixels = bytes[pixel_offset(&bytes)..].as_ptr();
    assert_eq!(
        texture.read_pixels().unwrap().bytes().as_ptr(),
        original_pixels
    );
    drop(bytes);
    drop(app);
    assert!(weak.upgrade().is_some(), "the texture retains the reader");
    {
        let area = Region::new(0, 1, 1, 1).unwrap();
        let mut cx = Context::from_waker(Waker::noop());
        let Poll::Ready(Ok(region)) = texture.poll_pixels_in(area, &mut cx) else {
            panic!("mapped pixels are resident");
        };
        assert_eq!(region.tiles().count(), 1);
        assert_eq!(region.pixels().unwrap().stride(), 8);
        assert_eq!(
            region.pixels().unwrap().bytes().as_ptr(),
            original_pixels.wrapping_add(8)
        );
        assert_eq!(region.get_pixel(0, 1).unwrap().b, 255);
    }
    let mut texture = Rc::try_unwrap(texture).ok().unwrap();
    assert!(matches!(
        texture.pixels_mut(),
        Err(RenderError::InvalidTexture)
    ));
    drop(texture);
    assert!(
        weak.upgrade().is_none(),
        "the last owner releases the mapping"
    );
}

#[test]
fn mapped_mirx_rejects_corruption_and_exact_length_mismatches() {
    for valid in [flat(), chunk()] {
        mapped_invalid(valid);
    }
}

fn mapped_invalid(valid: Rc<Vec<u8>>) {
    let mut corrupt_header = (*valid).clone();
    corrupt_header[12] ^= 1;
    let mut trailing = (*valid).clone();
    trailing.push(0);
    let truncated = valid[..valid.len() - 1].to_vec();
    for bytes in [corrupt_header, trailing, truncated] {
        for known_length in [false, true] {
            let control = Rc::new(Control::default());
            control.complete();
            let mut app = app(Rc::new(bytes.clone()), control, known_length);
            app.tick();
            let images = app
                .world
                .resource::<ResourceManager<Texture<'static>>>()
                .unwrap();
            assert_eq!(
                images.resolve("invalid").width(),
                16,
                "invalid sources use fallback"
            );
        }
    }
}

#[test]
fn mapped_chunk_probe_retains_the_same_source_until_value_publication() {
    let bytes = chunk();
    let control = Rc::new(Control::default());
    control.complete();
    let mut app = app(bytes, Rc::clone(&control), true);
    app.tick();
    let images = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    let meta = images.probe("cover").unwrap();
    assert_eq!((meta.width, meta.height), (1, 2));
    let first = images.resolve("cover");
    assert_eq!((first.width(), first.height()), (1, 2));
    assert_eq!(control.opens.get(), 1);
    images.remove_token("cover");
    let second = images.resolve("cover");
    assert_eq!(control.opens.get(), 2);
    assert_eq!(
        first.read_pixels().unwrap().bytes(),
        second.read_pixels().unwrap().bytes()
    );
}

#[test]
fn mapped_chunk_validates_critical_payloads_outside_the_selected_image() {
    let valid = chunk();
    let first = mirx::Reader::open(&valid)
        .unwrap()
        .chunks()
        .find(|chunk| chunk.chunk_type() == mirx::ChunkType::IMAGE && chunk.flags().is_critical())
        .unwrap();
    assert!(first.flags().is_critical());
    let offset = first.payload_offset() as usize;
    let mut corrupted = (*valid).clone();
    corrupted[offset] = 0xff;
    assert!(mirx::Reader::open(&corrupted).is_err());
    let control = Rc::new(Control::default());
    control.complete();
    let mut app = app(Rc::new(corrupted), control, true);
    app.tick();
    let images = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    assert_eq!(images.resolve("cover").width(), 16);
}

#[test]
fn mapped_source_does_not_promote_encoded_storage_to_implicit_full_decode() {
    use mirx::image::{ColorDescription, EncodedImageAsset, SampleLayout, SurfaceDescriptor};
    let codec = mirx::coding::Pixel::new(SampleLayout::RGB888).unwrap();
    let surface =
        SurfaceDescriptor::new(1, 1, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
    let samples = [255, 0, 0];
    let mut encoded = [0; 32];
    let len = codec.encode_into(&samples, &mut encoded).unwrap();
    let image = EncodedImageAsset::new(surface, codec.record(), &encoded[..len]);
    let mut document = mirx::Document::new();
    let primary = document.push_encoded_image(&image).unwrap();
    document.set_primary(primary).unwrap();
    let bytes = Rc::new(document.finish().unwrap().into_owned());
    assert!(matches!(
        mirx::Reader::open(&bytes)
            .unwrap()
            .primary()
            .unwrap()
            .unwrap()
            .image()
            .unwrap(),
        Some(mirx::image::ImageRef::Encoded(_))
    ));
    let control = Rc::new(Control::default());
    control.complete();
    let mut app = app(bytes, control, true);
    app.tick();
    let images = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    assert_eq!(images.resolve("cover").width(), 16);
}

#[derive(Default)]
struct MappedEngine {
    pointer: Option<*const u8>,
    calls: usize,
}

impl<T: Renderer> RenderEngine<T> for MappedEngine {
    fn prepare<'data>(
        &mut self,
        target: &mut T,
        request: DrawRequest<'_, 'data>,
        route: RenderRoute,
        cx: &mut Context<'_>,
    ) -> Poll<Result<PreparedDraw<'data>, RenderError>> {
        target.prepare(request, route, cx)
    }

    fn submit(&mut self, target: &mut T, draw: &PreparedDraw<'_>) -> Result<(), RenderError> {
        self.pointer = draw
            .texture_region()
            .and_then(|region| region.pixels())
            .map(|pixels| pixels.bytes().as_ptr());
        self.calls += 1;
        target.submit(draw)
    }
}

compose_backend! {
    struct MappedInner { target: SwRenderer, images: MappedEngine }
    route { default => target, blit => images }
}

compose_backend! {
    struct MappedOuter { target: MappedInner, images: MappedEngine }
    route { default => target, blit => images }
}

pub(super) fn compare_region_routes(texture: &Texture<'_>, direct: &Texture<'_>, area: Rect) {
    let mut results = Vec::new();
    for texture in [texture, direct] {
        let mut destination = [0; 32 * 32 * 4];
        {
            let mut renderer = MappedOuter::new(
                MappedInner::new(
                    SwRenderer::new(
                        Texture::new(&mut destination, 32, 32, ColorFormat::RGBA8888).unwrap(),
                    ),
                    MappedEngine::default(),
                ),
                MappedEngine::default(),
            );
            let command = DrawCommand::Blit {
                texture,
                pos: Point::ZERO,
                size: Point::new(32, 32),
                transform: Transform::IDENTITY,
                quad: None,
                opa: 255,
                radius: Fixed::ZERO,
                composite: CompositeMode::SourceOver,
            };
            let request = DrawRequest::new(&command, area);
            let route = renderer.route(&request).unwrap();
            let mut cx = Context::from_waker(Waker::noop());
            let prepared = (0..256)
                .find_map(|_| match renderer.prepare(request, route, &mut cx) {
                    Poll::Ready(result) => Some(result.unwrap()),
                    Poll::Pending => None,
                })
                .expect("bounded source becomes ready");
            renderer.submit(&prepared).unwrap();
            assert_eq!(renderer.images.calls, 1);
            assert_eq!(renderer.target.images.calls, 1);
            assert_eq!(
                tracking_allocator::tracked_allocations(|| {
                    let Poll::Ready(Ok(prepared)) = renderer.prepare(request, route, &mut cx)
                    else {
                        panic!("warm region");
                    };
                    renderer.submit(&prepared).unwrap();
                }),
                0
            );
        }
        results.push(destination);
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn mapped_and_static_pixels_share_nested_routes_without_hot_allocations() {
    for bytes in [flat(), chunk()] {
        mapped_nested_routes(bytes);
    }
}

fn mapped_nested_routes(bytes: Rc<Vec<u8>>) {
    let control = Rc::new(Control::default());
    control.complete();
    let mut app = app(Rc::clone(&bytes), control, true);
    app.tick();
    let images = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    let mapped = images.resolve("cover");
    let direct = Texture::from_static(&PIXELS, 1, 2, ColorFormat::RGBA8888)
        .unwrap()
        .with_stride(8)
        .unwrap();
    let mut results = Vec::new();
    for texture in [mapped.as_ref(), &direct] {
        let expected_pointer = texture.read_pixels().unwrap().bytes()[8..].as_ptr();
        let mut destination = [0; 8];
        {
            let mut renderer = MappedOuter::new(
                MappedInner::new(
                    SwRenderer::new(
                        Texture::new(&mut destination, 1, 2, ColorFormat::RGBA8888).unwrap(),
                    ),
                    MappedEngine::default(),
                ),
                MappedEngine::default(),
            );
            let command = DrawCommand::Blit {
                texture,
                pos: Point::ZERO,
                size: Point::new(1, 2),
                transform: Transform::IDENTITY,
                quad: None,
                opa: 255,
                radius: Fixed::ZERO,
                composite: CompositeMode::SourceOver,
            };
            let request = DrawRequest::new(&command, Rect::new(0, 1, 1, 1));
            assert_eq!(
                super::tracking_allocator::tracked_allocations(|| {
                    let mut cx = Context::from_waker(Waker::noop());
                    let route = renderer.route(&request).unwrap();
                    let Poll::Ready(Ok(prepared)) = renderer.prepare(request, route, &mut cx)
                    else {
                        panic!("mapped and static sources are resident");
                    };
                    renderer.submit(&prepared).unwrap();
                }),
                0
            );
            assert_eq!(renderer.images.calls, 1);
            assert_eq!(renderer.target.images.calls, 1);
            assert_eq!(renderer.images.pointer, Some(expected_pointer));
            assert_eq!(renderer.target.images.pointer, renderer.images.pointer);
        }
        results.push(destination);
    }
    assert_eq!(results[0], results[1]);
    assert_eq!(results[0], [0, 0, 0, 0, 0, 0, 255, 255]);
}
