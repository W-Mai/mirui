#![cfg(feature = "std")]

#[path = "source_loading/mapped.rs"]
mod mapped;

#[path = "support/tracking_allocator.rs"]
mod tracking_allocator;

use mirui::app::App;
use mirui::app::plugins::{ImageResourcesPlugin, ResourcePlugin};
use mirui::core::cache::MaxSize;
use mirui::core::resource::{
    AssetBuffer, AssetCacheConfig, AssetError, AssetReader, AssetSource, FormatMatch, LoadContext,
    LoadError, LoadInput, Loader, ProbeLoader, ReadComplete, ReadPosition, ReadProgress,
    ResourceManager, SourceAccess, SourceConsistency, SourceInfo,
};
use mirui::render::texture::{ColorFormat, Texture, TextureMeta};
use mirui::surface::{FramebufferAccess, framebuf::FramebufSurface};
use mirui::types::PhysicalRect;
use mirui::ui;
use mirui::ui::widgets::Image;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::future::{Future, ready};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

static RED: &[u8] = b"R4IM\x01\x00\x01\x00\xff\x00\x00\xff";
type Headless = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;

#[derive(Default)]
struct Control {
    ready: Cell<bool>,
    opens: Cell<usize>,
    reads: Cell<usize>,
    completions: Cell<usize>,
    wake: RefCell<Option<Waker>>,
    keys: RefCell<Vec<String>>,
}
impl Control {
    fn complete(&self) {
        self.ready.set(true);
        if let Some(wake) = self.wake.borrow_mut().take() {
            wake.wake();
        }
    }
}
struct Reader {
    control: Rc<Control>,
    cursor: Rc<Cell<usize>>,
    sequential: bool,
}
struct Read {
    control: Rc<Control>,
    cursor: Rc<Cell<usize>>,
    offset: usize,
    buffer: Option<AssetBuffer>,
}
impl Future for Read {
    type Output = ReadComplete;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.control.wake.replace(Some(cx.waker().clone()));
        if !self.control.ready.get() {
            return Poll::Pending;
        }
        let mut buffer = self
            .buffer
            .take()
            .expect("completed reads are not polled again");
        let rest = &RED[self.offset..];
        // Short physical reads must fill the same framework block without losing bytes.
        let filled = rest.len().min(buffer.len()).min(3);
        buffer.as_mut_slice()[..filled].copy_from_slice(&rest[..filled]);
        self.cursor.set(self.offset + filled);
        self.control
            .completions
            .set(self.control.completions.get() + 1);
        Poll::Ready(ReadComplete {
            buffer,
            result: Ok(ReadProgress {
                filled,
                eof: filled == rest.len(),
            }),
        })
    }
}
impl AssetReader for Reader {
    type Read = Read;
    fn info(&self) -> SourceInfo {
        SourceInfo {
            len: None,
            access: if self.sequential {
                SourceAccess::Sequential
            } else {
                SourceAccess::Random
            },
            consistency: SourceConsistency::Stable,
        }
    }
    fn read(&self, position: ReadPosition, buffer: AssetBuffer) -> Read {
        let offset = match position {
            ReadPosition::At(offset) => {
                assert!(!self.sequential);
                usize::try_from(offset).unwrap()
            }
            ReadPosition::Next => {
                assert!(self.sequential);
                self.cursor.get()
            }
        };
        self.control.reads.set(self.control.reads.get() + 1);
        Read {
            control: self.control.clone(),
            cursor: self.cursor.clone(),
            offset,
            buffer: Some(buffer),
        }
    }
}

#[derive(Default)]
struct Decode {
    header: [u8; 8],
    pixels: [u8; 4],
    probed: bool,
    pixels_read: bool,
}
struct Decoder;
impl Loader<Texture<'static>> for Decoder {
    type State = Decode;
    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch {
        match input {
            LoadInput::Bytes {
                prefix, eof: false, ..
            } if prefix.len() < 4 => FormatMatch::Need(4),
            LoadInput::Bytes { prefix, .. } if prefix.starts_with(b"R4IM") => FormatMatch::Match,
            _ => FormatMatch::NoMatch,
        }
    }
    fn poll_load(
        &self,
        state: &mut Decode,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<Texture<'static>, LoadError>> {
        core::task::ready!(self.poll_probe(state, cx))?;
        if !state.pixels_read {
            core::task::ready!(cx.poll_read_exact(8, &mut state.pixels))?;
            state.pixels_read = true;
        }
        // Strict formats distinguish a confirmed EOF from provider failures.
        if core::task::ready!(cx.poll_read(12, &mut [0]))? != 0 {
            return Poll::Ready(Err(LoadError::Failed("unexpected trailing bytes")));
        }
        Poll::Ready(
            Texture::from_vec(state.pixels.to_vec(), 1, 1, ColorFormat::RGBA8888)
                .map_err(|_| LoadError::Failed("invalid test texture")),
        )
    }
}
impl ProbeLoader<Texture<'static>> for Decoder {
    fn poll_probe(
        &self,
        state: &mut Decode,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<TextureMeta, LoadError>> {
        if !state.probed {
            core::task::ready!(cx.poll_read_exact(0, &mut state.header))?;
            if state.header != RED[..8] {
                return Poll::Ready(Err(LoadError::Failed("invalid header")));
            }
            state.probed = true;
        }
        Poll::Ready(Ok(TextureMeta {
            width: 1,
            height: 1,
            format: ColorFormat::RGBA8888,
        }))
    }
}

fn app(control: Rc<Control>, sequential: bool, duplicate: bool) -> Headless {
    let mut app = App::headless(8, 8);
    app.with_widget(mirui::ui::widgets::image::view());
    // Binding uses on_start, so registration order does not couple the two plugins.
    app.add_plugin(
        ImageResourcesPlugin::empty()
            .with_source("files", |token| Some(format!("pictures/{token}.r4").into()))
            .with_loader(move |images| {
                images.add_probed_loader(Decoder);
                if duplicate {
                    images.add_probed_loader(Decoder);
                }
            }),
    );
    app.add_plugin(
        ResourcePlugin::new()
            .with_cache(
                AssetCacheConfig::new()
                    .block_size(4)
                    .budget(MaxSize::Count(2))
                    .max_in_flight(1)
                    .max_requests(1)
                    .probe_bytes(8),
            )
            .with_source("files", move |key: Cow<'static, str>| {
                control.opens.set(control.opens.get() + 1);
                control.keys.borrow_mut().push(key.into_owned());
                ready(Ok::<_, AssetError>(Reader {
                    control: control.clone(),
                    cursor: Rc::new(Cell::new(0)),
                    sequential,
                }))
            }),
    );
    app
}

#[test]
fn source_completion_reaches_image_through_existing_resource_subscription() {
    for sequential in [false, true] {
        let control = Rc::new(Control::default());
        let mut app = app(control.clone(), sequential, false);
        let root = app.spawn_root().id();
        app.compose(root, |cx| {
            ui! { Image(src: "cover/42", width: 8, height: 8) }
        });
        app.tick();
        assert_eq!(control.opens.get(), 1);
        assert_eq!(control.reads.get(), 1);
        control.complete();
        for _ in 0..8 {
            app.tick();
        }
        assert_eq!(
            &app.backend.framebuffer().read_pixels().unwrap().bytes()[..4],
            &[255, 0, 0, 255]
        );
        assert_eq!(
            control.opens.get(),
            1,
            "metadata and pixels share the opened reader"
        );
        assert_eq!(&*control.keys.borrow(), &["pictures/cover/42.r4"]);
        let reads = control.reads.get();
        for _ in 0..8 {
            app.tick();
        }
        assert_eq!(control.reads.get(), reads, "cache hits do not restart I/O");
    }
}

#[test]
fn invalidation_retires_physical_io_before_reusing_the_source_slot() {
    let control = Rc::new(Control::default());
    let mut app = app(control.clone(), false, false);
    app.tick();
    let manager = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    assert_eq!(manager.resolve("old").width(), 16);
    assert_eq!(control.reads.get(), 1);
    manager.remove_token("old");
    app.tick();
    let manager = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    assert_eq!(manager.resolve("new").width(), 16);
    assert_eq!(
        control.opens.get(),
        1,
        "retiring source still occupies its prepared slot"
    );
    control.complete();
    for _ in 0..3 {
        app.tick();
    }
    assert_eq!(
        control.completions.get(),
        1,
        "cancellation cannot start another short read"
    );
    let manager = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    manager.remove_token("new");
    assert_eq!(manager.resolve("new").width(), 1);
    assert_eq!(control.opens.get(), 2);
}

#[test]
fn multiple_format_claims_fail_before_decoding() {
    let control = Rc::new(Control::default());
    control.ready.set(true);
    let mut app = app(control.clone(), true, true);
    app.tick();
    let manager = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    assert_eq!(manager.resolve("ambiguous").width(), 16);
    assert_eq!(
        control.completions.get(),
        2,
        "only the four-byte recognition prefix was read"
    );
}

struct ExplicitDecoder;
impl Loader<Texture<'static>> for ExplicitDecoder {
    type State = Decode;
    fn recognize(&self, _: LoadInput<'_>) -> FormatMatch {
        panic!("explicit source selection must not guess the content format");
    }
    fn poll_load(
        &self,
        state: &mut Decode,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<Texture<'static>, LoadError>> {
        Decoder.poll_load(state, cx)
    }
}
impl ProbeLoader<Texture<'static>> for ExplicitDecoder {
    fn poll_probe(
        &self,
        state: &mut Decode,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<TextureMeta, LoadError>> {
        Decoder.poll_probe(state, cx)
    }
}

fn mapped_app(plugin: ImageResourcesPlugin, opens: Rc<Cell<usize>>) -> Headless {
    let mut app = App::headless(8, 8);
    app.add_plugin(plugin);
    app.add_plugin(
        ResourcePlugin::new().with_source("files", move |_: Cow<'static, str>| {
            opens.set(opens.get() + 1);
            ready(Ok::<_, AssetError>(RED))
        }),
    );
    app.tick();
    app
}

#[test]
fn strict_source_decoders_reject_trailing_bytes_without_publishing_pixels() {
    for bytes in [RED, b"R4IM\x01\x00\x01\x00\xff\x00\x00\xffextra".as_slice()] {
        let mut app: Headless = App::headless(8, 8);
        app.add_plugin(ImageResourcesPlugin::empty().with_loader(|images| {
            images.add_probed_loader(Decoder.for_source("files", |_| Some("image".into())));
        }));
        app.add_plugin(ResourcePlugin::new().with_source("files", move |_| ready(Ok(bytes))));
        app.tick();
        let images = app
            .world
            .resource::<ResourceManager<Texture<'static>>>()
            .unwrap();
        assert_eq!(images.probe("image").unwrap().width, 1);
        assert_eq!(
            images.resolve("image").width(),
            if bytes.len() == RED.len() { 1 } else { 16 }
        );
    }
}

#[test]
fn explicit_binding_reuses_metadata_and_value_state_without_recognition() {
    let opens = Rc::new(Cell::new(0));
    let app = mapped_app(
        ImageResourcesPlugin::empty().with_loader(|images| {
            images.add_probed_loader(ExplicitDecoder.for_source("files", |token| {
                (token == "cover").then(|| "raw/image".into())
            }));
        }),
        opens.clone(),
    );
    let manager = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    assert_eq!(manager.probe("cover").unwrap().width, 1);
    assert_eq!(
        manager.resolve("cover").read_pixels().unwrap().bytes(),
        &[255, 0, 0, 255]
    );
    assert_eq!(opens.get(), 1);
    assert_eq!(manager.resolve("unclaimed").width(), 16);
    assert_eq!(opens.get(), 1);
}

#[test]
fn explicit_source_conflicts_fail_before_opening_in_either_registration_order() {
    for reverse in [false, true] {
        for automatic in [false, true] {
            let opens = Rc::new(Cell::new(0));
            let mut plugin = ImageResourcesPlugin::empty();
            if automatic {
                plugin = plugin.with_source("files", |_| Some("automatic".into()));
            }
            plugin = plugin.with_loader(move |images| {
                if reverse && !automatic {
                    images.add_loader(Decoder.for_source("files", |_| Some("other".into())));
                }
                images.add_probed_loader(
                    ExplicitDecoder.for_source("files", |_| Some("explicit".into())),
                );
                if !reverse && !automatic {
                    images.add_loader(Decoder.for_source("files", |_| Some("other".into())));
                }
            });
            let app = mapped_app(plugin, opens.clone());
            let manager = app
                .world
                .resource::<ResourceManager<Texture<'static>>>()
                .unwrap();
            assert_eq!(manager.resolve("ambiguous").width(), 16);
            assert_eq!(opens.get(), 0);
        }
    }
}

#[test]
fn automatic_recognition_does_not_consult_explicit_decoders() {
    let app = mapped_app(
        ImageResourcesPlugin::empty()
            .with_source("files", |token| {
                (token == "automatic").then(|| "auto".into())
            })
            .with_loader(|images| {
                images.add_probed_loader(
                    ExplicitDecoder
                        .for_source("files", |token| (token == "explicit").then(|| "raw".into())),
                );
                images.add_probed_loader(Decoder);
            }),
        Rc::new(Cell::new(0)),
    );
    let manager = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    assert_eq!(manager.resolve("automatic").width(), 1);
    assert_eq!(manager.resolve("explicit").width(), 1);
}

#[test]
fn bound_loader_without_probe_never_decodes_for_metadata() {
    let opens = Rc::new(Cell::new(0));
    let app = mapped_app(
        ImageResourcesPlugin::empty().with_loader(|images| {
            images.add_loader(ExplicitDecoder.for_source("files", |_| Some("raw".into())));
        }),
        opens.clone(),
    );
    let manager = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    assert!(manager.probe("cover").is_none());
    assert_eq!(manager.resolve("cover").width(), 1);
}

#[test]
#[should_panic(expected = "unknown image byte source")]
fn explicit_binding_validates_provider_before_loading() {
    mapped_app(
        ImageResourcesPlugin::empty().with_loader(|images| {
            images.add_loader(Decoder.for_source("missing", |_| None));
        }),
        Rc::new(Cell::new(0)),
    );
}

#[test]
fn last_consumer_retirement_returns_submitted_reads_without_continuing_decode() {
    let control = Rc::new(Control::default());
    let mut app = app(control.clone(), true, false);
    app.tick();
    let handle = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap()
        .load("shared");
    let second = handle.clone();
    assert_eq!(handle.get().width(), 16);
    assert_eq!(control.reads.get(), 1);
    drop(handle);
    app.tick();
    drop(second);
    app.tick();
    control.complete();
    for _ in 0..4 {
        app.tick();
    }
    assert_eq!(control.completions.get(), 1);
    assert_eq!(
        control.reads.get(),
        1,
        "retirement cannot issue the next short read"
    );
    let manager = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    let replacement = manager.load("shared");
    assert_eq!(replacement.get().width(), 1);
    assert_eq!(
        control.opens.get(),
        2,
        "the source slot was returned after physical retirement"
    );
}

#[test]
fn image_changes_release_pending_demand_before_the_next_visible_paint() {
    for change in 0..4 {
        let control = Rc::new(Control::default());
        let mut app = app(control.clone(), true, false);
        let root = app.spawn_root().id();
        let image = app.compose(root, |cx| {
            ui! { Image(src: "pending", width: 8, height: 8) }
        });
        app.tick();
        assert_eq!(control.reads.get(), 1);
        match change {
            0 => {
                app.world.remove::<Image>(image);
            }
            1 => {
                app.world.despawn(image);
            }
            2 => {
                app.world.get_mut::<Image>(image).unwrap().src =
                    mirui::render::path::Path::new().into();
            }
            _ => {
                app.world
                    .resource::<ResourceManager<Texture<'static>>>()
                    .unwrap()
                    .add_static(
                        "ready",
                        Texture::from_static(&RED[8..], 1, 1, ColorFormat::RGBA8888).unwrap(),
                    );
                app.world.get_mut::<Image>(image).unwrap().src = "ready".into();
            }
        }
        // Querying the dirty plan reconciles component dependencies without painting.
        app.dirty_regions().unwrap();
        app.tick();
        control.complete();
        for _ in 0..4 {
            app.tick();
        }
        assert_eq!(control.completions.get(), 1);
        assert_eq!(
            control.reads.get(),
            1,
            "retired images must not continue short reads"
        );
    }
}

struct RetainSource(Rc<RefCell<Option<AssetSource>>>);
impl Loader<Texture<'static>> for RetainSource {
    type State = ();
    fn recognize(&self, _: LoadInput<'_>) -> FormatMatch {
        FormatMatch::NoMatch
    }
    fn poll_load(
        &self,
        _: &mut (),
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<Texture<'static>, LoadError>> {
        let source = cx.take_source()?;
        assert!(cx.source_info().is_none());
        assert!(matches!(
            cx.take_source(),
            Err(LoadError::Source(AssetError::InvalidSource))
        ));
        *self.0.borrow_mut() = Some(source);
        Poll::Ready(Ok(Texture::from_static(
            &RED[8..],
            1,
            1,
            ColorFormat::RGBA8888,
        )
        .unwrap()))
    }
}

#[derive(Default)]
struct OwnedDecode {
    source: Option<AssetSource>,
    bytes: [u8; 12],
    filled: usize,
}
struct OwnedDecoder;
impl Loader<Texture<'static>> for OwnedDecoder {
    type State = OwnedDecode;
    fn recognize(&self, _: LoadInput<'_>) -> FormatMatch {
        FormatMatch::NoMatch
    }
    fn poll_load(
        &self,
        state: &mut OwnedDecode,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<Texture<'static>, LoadError>> {
        if state.source.is_none() {
            state.source = Some(cx.take_source()?);
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        while state.filled < state.bytes.len() {
            if !cx.consume_work() {
                return Poll::Pending;
            }
            let count = core::task::ready!(state.source.as_mut().unwrap().poll_read(
                state.filled as u64,
                &mut state.bytes[state.filled..],
                &mut Context::from_waker(cx.waker()),
            ))?;
            if count == 0 {
                return Poll::Ready(Err(AssetError::Truncated.into()));
            }
            state.filled += count;
        }
        assert_eq!(&state.bytes, RED);
        Poll::Ready(
            Texture::from_vec(state.bytes[8..].to_vec(), 1, 1, ColorFormat::RGBA8888)
                .map_err(|_| AssetError::InvalidSource.into()),
        )
    }
}

#[test]
fn transferred_source_state_resumes_and_retires_through_original_loader_slots() {
    for cancel in [false, true] {
        let control = Rc::new(Control::default());
        let provider = control.clone();
        let mut app = App::headless(8, 8);
        app.add_plugin(ImageResourcesPlugin::empty().with_loader(|images| {
            images.add_loader(OwnedDecoder.for_source("files", |_| Some("object".into())));
        }));
        app.add_plugin(
            ResourcePlugin::new().with_source("files", move |_: Cow<'static, str>| {
                provider.opens.set(provider.opens.get() + 1);
                ready(Ok::<_, AssetError>(Reader {
                    control: provider.clone(),
                    cursor: Rc::new(Cell::new(0)),
                    sequential: true,
                }))
            }),
        );
        app.tick();
        let handle = app
            .world
            .resource::<ResourceManager<Texture<'static>>>()
            .unwrap()
            .load("data");
        assert_eq!(handle.get().width(), 16);
        app.tick();
        assert_eq!(control.reads.get(), 1);
        let handle = if cancel {
            drop(handle);
            None
        } else {
            Some(handle)
        };
        app.tick();
        control.complete();
        for _ in 0..4 {
            app.tick();
        }
        if let Some(handle) = handle {
            assert_eq!(handle.get().read_pixels().unwrap().bytes(), &RED[8..]);
        } else {
            assert_eq!(control.reads.get(), 1);
            assert_eq!(control.completions.get(), 1);
        }
        assert_eq!(control.opens.get(), 1);
    }
}

#[test]
fn transferred_source_survives_loader_manager_and_app_without_reopening() {
    let control = Rc::new(Control::default());
    let provider = control.clone();
    let retained = Rc::new(RefCell::new(None));
    let output = retained.clone();
    let mut app = App::headless(8, 8);
    app.add_plugin(ImageResourcesPlugin::empty().with_loader(move |images| {
        images.add_loader(RetainSource(output).for_source("files", |_| Some("object".into())));
    }));
    app.add_plugin(
        ResourcePlugin::new().with_source("files", move |_: Cow<'static, str>| {
            provider.opens.set(provider.opens.get() + 1);
            ready(Ok::<_, AssetError>(Reader {
                control: provider.clone(),
                cursor: Rc::new(Cell::new(0)),
                sequential: true,
            }))
        }),
    );
    app.tick();
    app.world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap()
        .resolve("data");
    assert_eq!(control.opens.get(), 1);
    assert_eq!(
        control.reads.get(),
        0,
        "transfer need not read the complete object"
    );
    let mut source = retained.borrow_mut().take().unwrap();
    drop(app);
    assert_eq!(source.info().unwrap().access, SourceAccess::Sequential);
    let mut bytes = [0; 12];
    let mut cx = Context::from_waker(Waker::noop());
    assert!(source.poll_read(0, &mut bytes, &mut cx).is_pending());
    control.complete();
    assert_eq!(
        source.poll_read(0, &mut bytes, &mut cx),
        Poll::Ready(Ok(12))
    );
    assert_eq!(&bytes, RED);
    assert_eq!(control.opens.get(), 1);
}

#[test]
fn refreshing_an_alias_reopens_its_resolved_key_without_mutating_old_pixels() {
    static BLUE: &[u8] = b"R4IM\x01\x00\x01\x00\x00\x00\xff\xff";
    let selected = Rc::new(Cell::new(false));
    let resolver = selected.clone();
    let opens = Rc::new(Cell::new(0));
    let count = opens.clone();
    let mut app = App::headless(8, 8);
    app.add_plugin(
        ResourcePlugin::new().with_source("files", move |key: Cow<'static, str>| {
            count.set(count.get() + 1);
            ready(Ok::<_, AssetError>(if key == "blue" { BLUE } else { RED }))
        }),
    );
    app.add_plugin(
        ImageResourcesPlugin::empty()
            .with_source("files", move |_| {
                Some(if resolver.get() { "blue" } else { "red" }.into())
            })
            .with_loader(|images| images.add_probed_loader(Decoder)),
    );
    app.tick();
    let manager = app
        .world
        .resource::<ResourceManager<Texture<'static>>>()
        .unwrap();
    let handle = manager.load("alias");
    let old = handle.get();
    assert_eq!(old.read_pixels().unwrap().bytes(), &[255, 0, 0, 255]);
    selected.set(true);
    manager.reload("alias").unwrap();
    assert_eq!(opens.get(), 1);
    assert!(Rc::ptr_eq(&old, &handle.get()));
    app.tick();
    assert_eq!(opens.get(), 2);
    assert_eq!(
        handle.get().read_pixels().unwrap().bytes(),
        &[0, 0, 255, 255]
    );
    assert_eq!(old.read_pixels().unwrap().bytes(), &[255, 0, 0, 255]);
}
