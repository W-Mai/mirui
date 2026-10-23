use super::*;
use mirui::render::texture::{Region, TextureRegion};
use mirui::render::{RenderError, RenderResource};
use mirui::types::Color;

struct File {
    bytes: Rc<Vec<u8>>,
    control: Rc<Control>,
}

struct ReadFile {
    bytes: Rc<Vec<u8>>,
    control: Rc<Control>,
    offset: usize,
    buffer: Option<AssetBuffer>,
}

impl Future for ReadFile {
    type Output = ReadComplete;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.control.ready.get() {
            self.control.wake.replace(Some(cx.waker().clone()));
            return Poll::Pending;
        }
        let mut buffer = self.buffer.take().unwrap();
        let remaining = self.bytes.len().saturating_sub(self.offset);
        let filled = remaining.min(buffer.len()).min(13);
        buffer.as_mut_slice()[..filled]
            .copy_from_slice(&self.bytes[self.offset..self.offset + filled]);
        self.control
            .completions
            .set(self.control.completions.get() + 1);
        Poll::Ready(ReadComplete {
            buffer,
            result: Ok(ReadProgress {
                filled,
                eof: filled == remaining,
            }),
        })
    }
}

impl AssetReader for File {
    type Read = ReadFile;

    fn info(&self) -> SourceInfo {
        SourceInfo {
            len: Some(self.bytes.len() as u64),
            access: SourceAccess::Random,
            consistency: SourceConsistency::Stable,
        }
    }

    fn read(&self, position: ReadPosition, buffer: AssetBuffer) -> Self::Read {
        let ReadPosition::At(offset) = position else {
            panic!("random source");
        };
        self.control.reads.set(self.control.reads.get() + 1);
        ReadFile {
            bytes: self.bytes.clone(),
            control: self.control.clone(),
            offset: offset as usize,
            buffer: Some(buffer),
        }
    }
}

fn fixture(format: ColorFormat) -> Rc<Vec<u8>> {
    let mut pixels = Texture::owned(32, 32, format).unwrap();
    for y in 0..32 {
        for x in 0..32 {
            pixels.pixels_mut().unwrap().set_pixel(
                x,
                y,
                &Color::rgb((x * 8) as u8, (y * 8) as u8, 255),
            );
        }
    }
    let format = match format {
        ColorFormat::RGB565 => mirx::image::ColorFormat::RGB565,
        ColorFormat::RGB565Swapped => mirx::image::ColorFormat::RGB565Swapped,
        ColorFormat::RGB888 => mirx::image::ColorFormat::RGB888,
        ColorFormat::RGBA8888 => mirx::image::ColorFormat::RGBA8888,
        ColorFormat::BGRA8888 => mirx::image::ColorFormat::BGRA8888,
    };
    let stride = 32 * pixels.format().bytes_per_pixel() + 8;
    let mut padded = vec![0xa5; stride * 32];
    let source = pixels.read_pixels().unwrap();
    for row in 0..32 {
        padded[row * stride..row * stride + source.stride()]
            .copy_from_slice(&source.bytes()[row * source.stride()..(row + 1) * source.stride()]);
    }
    Rc::new(
        mirx::Document::new_flat(mirx::image::ImageAsset::new(
            32,
            32,
            format,
            stride as u32,
            Cow::Owned(padded),
        ))
        .unwrap()
        .finish()
        .unwrap()
        .into_owned(),
    )
}

fn app(bytes: Rc<Vec<u8>>, control: Rc<Control>, budget: usize) -> Headless {
    let mut app = App::headless(32, 32);
    app.add_plugin(
        ImageResourcesPlugin::empty()
            .with_decode_budget(budget)
            .with_source("files", |token| Some(token.to_owned().into())),
    );
    app.add_plugin(
        ResourcePlugin::new()
            .with_cache(
                AssetCacheConfig::new()
                    .block_size(64)
                    .budget(MaxSize::Count(2))
                    .probe_bytes(8)
                    .max_in_flight(1)
                    .max_requests(4),
            )
            .with_source("files", move |_| {
                control.opens.set(control.opens.get() + 1);
                ready(Ok(File {
                    bytes: bytes.clone(),
                    control: control.clone(),
                }))
            }),
    );
    app.tick();
    app
}

fn resolve(app: &Headless, name: &str) -> Rc<Texture<'static>> {
    let images = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..16 {
        let value = images.resolve(name);
        if value.width() == 32 {
            return value;
        }
        images.poll_pending(&mut cx);
    }
    panic!("source metadata did not resolve");
}

fn region<'a>(texture: &'a Texture<'_>, area: Region) -> Result<TextureRegion<'a>, RenderError> {
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..256 {
        if let Poll::Ready(result) = texture.poll_pixels_in(area, &mut cx) {
            return result;
        }
    }
    panic!("ready source did not finish bounded region reads");
}

#[test]
fn source_regions_preserve_padded_geometry_and_warm_reads_do_not_allocate() {
    for format in [
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ] {
        let bytes = fixture(format);
        let stride = 32 * format.bytes_per_pixel() + 8;
        let reference = Texture::from_ref(&bytes[bytes.len() - stride * 32..], 32, 32, format)
            .unwrap()
            .with_stride(stride)
            .unwrap();
        let control = Rc::new(Control::default());
        control.ready.set(true);
        let app = app(bytes.clone(), control.clone(), 8 * 1024);
        let texture = resolve(&app, "image");
        let before = control.reads.get();
        assert!(texture.pixels().unwrap().is_none());
        assert!(matches!(
            texture.read_pixels(),
            Err(RenderError::ResourcePending)
        ));
        assert_eq!(before, control.reads.get());
        let area = Region::new(3, 13, 7, 4).unwrap();
        let read = region(&texture, area).unwrap();
        for y in area.y()..area.bottom() {
            for x in area.x()..area.right() {
                assert_eq!(
                    read.get_pixel(x, y),
                    Some(
                        reference
                            .read_pixels()
                            .unwrap()
                            .get_pixel(x as i32, y as i32)
                    )
                );
            }
        }
        assert_eq!(
            read.pixels().unwrap().stride(),
            7 * format.bytes_per_pixel()
        );
        assert!(
            texture.pixels().unwrap().is_none(),
            "partial reads are not whole-image residency"
        );
        let before = control.reads.get();
        let allocations = tracking_allocator::tracked_allocations(|| {
            for _ in 0..8 {
                let subset = region(&texture, Region::new(4, 14, 2, 2).unwrap()).unwrap();
                assert_eq!(subset.get_pixel(4, 14), read.get_pixel(4, 14));
            }
        });
        assert_eq!(allocations, 0);
        assert_eq!(before, control.reads.get());
        assert!(
            control.completions.get() * 13 < bytes.len(),
            "regional preparation must not read the complete image"
        );
        super::mapped::compare_region_routes(
            &texture,
            &reference,
            mirui::types::Rect::new(3, 13, 7, 4),
        );
        drop(app);
        assert_eq!(
            region(&texture, area).unwrap().get_pixel(4, 14),
            read.get_pixel(4, 14)
        );
    }
}

#[test]
fn pending_pixels_remain_private_and_changed_requests_finish_original_io() {
    let control = Rc::new(Control::default());
    control.ready.set(true);
    let app = app(fixture(ColorFormat::RGBA8888), control.clone(), 16 * 1024);
    let texture = resolve(&app, "image");
    let first = Region::new(3, 13, 7, 4).unwrap();
    let second = Region::new(21, 24, 4, 3).unwrap();
    control.ready.set(false);
    let mut cx = Context::from_waker(Waker::noop());
    assert!(texture.poll_pixels_in(first, &mut cx).is_pending());
    assert!(texture.pixels_in(first).unwrap().is_none());
    assert!(texture.poll_pixels_in(second, &mut cx).is_pending());
    control.complete();
    let ready = region(&texture, second).unwrap();
    assert_eq!(ready.get_pixel(21, 24), Some(Color::rgb(168, 192, 255)));
    assert!(texture.pixels_in(first).unwrap().is_some());
    assert_eq!(control.opens.get(), 1);
}

#[test]
fn all_images_share_live_budget_and_pinned_evictions_remain_immutable() {
    let control = Rc::new(Control::default());
    control.ready.set(true);
    let app = app(fixture(ColorFormat::RGBA8888), control.clone(), 4096);
    let a = resolve(&app, "a");
    let b = resolve(&app, "b");
    let area = Region::new(0, 0, 24, 24).unwrap();
    let old = region(&a, area).unwrap();
    let pixel = old.get_pixel(23, 23);
    assert!(matches!(
        region(&b, area),
        Err(RenderError::ResourceLimit(RenderResource::Texture))
    ));
    assert_eq!(old.get_pixel(23, 23), pixel);
    drop(old);
    let new = region(&b, area).unwrap();
    assert_eq!(new.get_pixel(23, 23), pixel);
    assert!(matches!(
        region(&a, Region::new(0, 0, 32, 32).unwrap()),
        Err(RenderError::ResourceLimit(RenderResource::Texture))
    ));
    drop(app);
    assert_eq!(new.get_pixel(23, 23), pixel);
}

#[test]
fn zero_decode_budget_preserves_mapped_and_static_fast_paths() {
    let bytes = fixture(ColorFormat::RGBA8888);
    let bytes: &'static [u8] = Box::leak(bytes.as_ref().clone().into_boxed_slice());
    let mut app: Headless = App::headless(32, 32);
    app.add_plugin(
        ImageResourcesPlugin::empty()
            .with_decode_budget(0)
            .with_source("mapped", |_| Some("image".into())),
    );
    app.add_plugin(ResourcePlugin::new().with_source("mapped", move |_| ready(Ok(bytes))));
    app.tick();
    let image = resolve(&app, "image");
    assert!(image.pixels().unwrap().is_some());
}

#[test]
fn dropping_a_pending_texture_retires_its_original_read_without_more_rows() {
    let control = Rc::new(Control::default());
    control.ready.set(true);
    let mut app = app(fixture(ColorFormat::RGBA8888), control.clone(), 8192);
    let texture = resolve(&app, "image");
    control.ready.set(false);
    let mut cx = Context::from_waker(Waker::noop());
    assert!(
        texture
            .poll_pixels_in(Region::new(3, 13, 7, 4).unwrap(), &mut cx)
            .is_pending()
    );
    let submitted = control.reads.get();
    let completions = control.completions.get();
    app.world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap()
        .remove_token("image");
    drop(texture);
    app.tick();
    assert_eq!(control.completions.get(), completions);
    control.complete();
    for _ in 0..4 {
        app.tick();
    }
    assert_eq!(
        control.reads.get(),
        submitted,
        "retirement must not submit another physical short read"
    );
    assert_eq!(control.completions.get(), completions + 1);
    assert_eq!(resolve(&app, "replacement").width(), 32);
}
