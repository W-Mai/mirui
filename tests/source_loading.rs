#![cfg(feature = "std")]

use mirui::app::App;
use mirui::app::plugins::{ImageResourcesPlugin, ResourcePlugin};
use mirui::core::cache::MaxSize;
use mirui::core::resource::{
    AssetBuffer, AssetCacheConfig, AssetError, AssetReader, FormatMatch, LoadContext, LoadError,
    LoadInput, Loader, ProbeLoader, ReadComplete, ReadPosition, ReadProgress, ResourceManager,
    SourceAccess, SourceConsistency, SourceInfo,
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
        core::task::ready!(cx.poll_read_exact(8, &mut state.pixels))?;
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
