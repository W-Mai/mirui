#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
use crate::core::threading::Shared;
use alloc::boxed::Box;
use core::cell::RefCell;
use core::ops::Range;
use core::task::{Poll, ready};

use super::{TexBuf, Texture, TextureMeta, map_mirx_format, open_mirx_image, texture_from_surface};
use crate::core::resource::{
    AssetError, AssetSource, FormatMatch, LoadContext, LoadError, LoadInput, Loader, ProbeLoader,
};
use crate::render::{PlaneLayout, PlaneRequirements};
use mirx::reader::{ContainerLayout, FlatImageLayout, ReadOptions};

mod chunk;

pub(super) struct MappedTexture {
    source: AssetSource,
    range: Range<usize>,
    bytes: usize,
    #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
    identity: Shared<()>,
}

impl MappedTexture {
    pub(super) fn pixels(&self) -> Option<&[u8]> {
        self.source.mapped().ok()??.get(self.range.clone())
    }

    pub(super) fn cache_size(&self) -> usize {
        let bytes = self.bytes.saturating_add(core::mem::size_of::<Self>());
        #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
        let bytes = bytes.saturating_add(2 * core::mem::size_of::<usize>());
        bytes
    }

    #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
    pub(super) fn identity(&self) -> Shared<()> {
        Shared::clone(&self.identity)
    }

    fn into_texture(
        source: AssetSource,
        image: SourceImage,
    ) -> Result<Texture<'static>, LoadError> {
        let layout = image.layout;
        let bytes = source.mapped()?.ok_or(AssetError::Unsupported)?.len();
        if bytes != image.logical_len {
            return Err(AssetError::InvalidSource.into());
        }
        let range = image.range;
        if range.len() < layout.minimum_bytes() || range.end > bytes {
            return Err(AssetError::Truncated.into());
        }
        Ok(Texture::from_layout(
            TexBuf::Mapped(Box::new(Self {
                source,
                range,
                bytes,
                #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
                identity: Shared::new(()),
            })),
            layout,
            false,
        ))
    }
}

#[cfg(all(test, feature = "std", any(feature = "sdl-gpu", feature = "wgpu")))]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::app::plugins::{ImageResourcesPlugin, ResourcePlugin};
    use crate::core::resource::ResourceManager;
    use crate::render::texture::TextureKey;
    use crate::surface::framebuf::FramebufSurface;
    use crate::types::PhysicalRect;
    use alloc::borrow::Cow;
    use core::future::ready;

    type TestApp = App<FramebufSurface<fn(&[u8], PhysicalRect)>>;

    #[test]
    fn mapped_opens_have_distinct_upload_identity_even_for_the_same_address() {
        let bytes = mirx::Document::new_flat(mirx::image::ImageAsset::new(
            1,
            1,
            mirx::image::ColorFormat::RGB565,
            2,
            Cow::Borrowed(&[0, 0]),
        ))
        .unwrap()
        .finish()
        .unwrap()
        .into_owned();
        use crate::core::threading::Shared;
        struct Mapping(Shared<alloc::vec::Vec<u8>>);
        impl crate::core::resource::AssetReader for Mapping {
            type Read = core::future::Ready<crate::core::resource::ReadComplete>;
            fn info(&self) -> crate::core::resource::SourceInfo {
                crate::core::resource::SourceInfo {
                    len: Some(self.0.len() as u64),
                    access: crate::core::resource::SourceAccess::Random,
                    consistency: crate::core::resource::SourceConsistency::Stable,
                }
            }
            fn mapped(&self) -> Option<&[u8]> {
                Some(&self.0)
            }
            fn read(
                &self,
                _: crate::core::resource::ReadPosition,
                _: crate::core::resource::AssetBuffer,
            ) -> Self::Read {
                panic!("mapped source cannot require physical reads");
            }
        }
        let bytes = Shared::new(bytes);
        let mut app: TestApp = App::headless(1, 1);
        app.add_plugin(
            ImageResourcesPlugin::empty().with_source("mapped", |_| Some("same".into())),
        );
        app.add_plugin(
            ResourcePlugin::new()
                .with_source("mapped", move |_| ready(Ok(Mapping(Shared::clone(&bytes))))),
        );
        app.tick();
        let images = app
            .world
            .resource::<ResourceManager<Texture<'static>>>()
            .unwrap();
        let first = images.resolve("first");
        let second = images.resolve("second");
        assert_eq!(
            first.read_pixels().unwrap().bytes().as_ptr(),
            second.read_pixels().unwrap().bytes().as_ptr()
        );
        let old = TextureKey::cacheable(&first).unwrap();
        assert_eq!(old, TextureKey::cacheable(&first).unwrap());
        assert_ne!(old, TextureKey::cacheable(&second).unwrap());
        images.remove_token("first");
        drop(first);
        let replacement = images.resolve("first");
        assert_ne!(old, TextureKey::cacheable(&replacement).unwrap());
    }
}

fn layout(image: FlatImageLayout) -> Result<PlaneLayout, LoadError> {
    let format = map_mirx_format(image.format())?;
    let width =
        u16::try_from(image.width()).map_err(|_| LoadError::Failed("MIRX width exceeds u16"))?;
    let height =
        u16::try_from(image.height()).map_err(|_| LoadError::Failed("MIRX height exceeds u16"))?;
    let stride = usize::try_from(image.stride()).map_err(|_| AssetError::OutOfBounds)?;
    PlaneLayout::new(width, height, format, stride, PlaneRequirements::CPU)
        .map_err(|_| LoadError::Failed("invalid MIRX pixel layout"))
}

pub(crate) struct SourceMirxLoader {
    decode_budget: usize,
    pool: RefCell<Option<super::decoded::Pool>>,
}

impl SourceMirxLoader {
    pub(crate) fn new(decode_budget: usize) -> Self {
        Self {
            decode_budget,
            pool: RefCell::new(None),
        }
    }

    fn pool(&self) -> Result<super::decoded::Pool, LoadError> {
        let mut pool = self.pool.borrow_mut();
        if pool.is_none() {
            *pool = Some(
                super::decoded::DecodePool::new(self.decode_budget)
                    .map_err(|_| AssetError::Capacity)?,
            );
        }
        Ok(pool.as_ref().expect("prepared decode pool").clone())
    }
}

struct SourceImage {
    layout: PlaneLayout,
    range: Range<usize>,
    logical_len: usize,
    encoded: Option<Box<super::decoded::EncodedImage>>,
}

impl SourceImage {
    fn mapped(bytes: &[u8]) -> Result<Option<Self>, LoadError> {
        let mirx::image::ImageRef::Raw(surface) = open_mirx_image(bytes)? else {
            return Ok(None);
        };
        let texture = texture_from_surface(surface)?;
        let layout = texture
            .storage_layout()
            .map_err(super::MirxLoadError::Texture)?;
        let pixels = texture
            .read_pixels()
            .map_err(super::MirxLoadError::Texture)?;
        // The checked reader supplies a plane inside the original immutable mapping.
        let start = (pixels.bytes().as_ptr() as usize)
            .checked_sub(bytes.as_ptr() as usize)
            .ok_or(AssetError::InvalidSource)?;
        let end = start
            .checked_add(pixels.bytes().len())
            .filter(|end| *end <= bytes.len())
            .ok_or(AssetError::InvalidSource)?;
        Ok(Some(Self {
            layout,
            range: start..end,
            logical_len: bytes.len(),
            encoded: None,
        }))
    }
}

pub(crate) struct SourceMirxState {
    header: [u8; 44],
    header_len: Option<usize>,
    container: Option<ContainerLayout>,
    chunk: Option<Box<chunk::ChunkRead>>,
    image: Option<SourceImage>,
    source: Option<AssetSource>,
    length_check: LengthCheck,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LengthCheck {
    Unchecked,
    EndPresent,
    Complete,
}

impl LengthCheck {
    fn poll(&mut self, end: u64, cx: &mut LoadContext<'_>) -> Poll<Result<(), LoadError>> {
        let mut byte = [0u8; 1];
        if *self == Self::Unchecked {
            ready!(cx.poll_read_exact(
                end.checked_sub(1).ok_or(AssetError::InvalidSource)?,
                &mut byte
            ))?;
            *self = Self::EndPresent;
        }
        if ready!(cx.poll_read(end, &mut byte))? != 0 {
            return Poll::Ready(Err(AssetError::InvalidSource.into()));
        }
        Poll::Ready(Ok(()))
    }
}

impl Default for SourceMirxState {
    fn default() -> Self {
        Self {
            header: [0; 44],
            header_len: None,
            container: None,
            chunk: None,
            image: None,
            source: None,
            length_check: LengthCheck::Unchecked,
        }
    }
}

impl Loader<Texture<'static>> for SourceMirxLoader {
    type State = SourceMirxState;

    fn recognize(&self, input: LoadInput<'_>) -> FormatMatch {
        let LoadInput::Bytes { prefix, eof, .. } = input else {
            return FormatMatch::NoMatch;
        };
        if prefix.len() < 4 {
            return if eof {
                FormatMatch::NoMatch
            } else {
                FormatMatch::Need(4)
            };
        }
        if prefix.starts_with(b"MIRX") {
            FormatMatch::Match
        } else {
            FormatMatch::NoMatch
        }
    }

    fn poll_load(
        &self,
        state: &mut Self::State,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<Texture<'static>, LoadError>> {
        ready!(self.poll_probe(state, cx))?;
        if state.source.is_none() {
            state.source = Some(cx.take_source()?);
        }
        if state
            .image
            .as_ref()
            .expect("probed image")
            .encoded
            .is_none()
            && state.source.as_ref().unwrap().mapped()?.is_some()
        {
            let image = state.image.take().expect("probed image");
            let source = state.source.take().unwrap();
            Poll::Ready(MappedTexture::into_texture(source, image))
        } else {
            let pool = self.pool()?;
            let mut storage =
                ready!(pool.poll_take(&mut core::task::Context::from_waker(cx.waker())))
                    .map_err(|_| AssetError::Capacity)?;
            let image = state.image.take().expect("probed image");
            let source = state.source.take().unwrap();
            Poll::Ready(super::decoded::DecodedTexture::into_texture(
                source,
                image.layout,
                image.range.start,
                pool.clone(),
                &mut storage,
                image.encoded,
            ))
        }
    }
}

impl ProbeLoader<Texture<'static>> for SourceMirxLoader {
    fn poll_probe(
        &self,
        state: &mut Self::State,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<TextureMeta, LoadError>> {
        if state.container.is_none() {
            if state.header_len.is_none() {
                ready!(cx.poll_read_exact(0, &mut state.header[..8]))?;
                let len = ContainerLayout::header_len(&state.header[..8])
                    .map_err(|_| LoadError::Failed("invalid MIRX header"))?;
                if len > state.header.len() {
                    return Poll::Ready(Err(AssetError::Unsupported.into()));
                }
                state.header_len = Some(len);
            }
            let len = state.header_len.expect("parsed header length");
            ready!(cx.poll_read_exact(8, &mut state.header[8..len]))?;
            let container = ContainerLayout::parse(&state.header[..len], &ReadOptions::default())
                .map_err(|_| LoadError::Failed("invalid MIRX container"))?;
            if cx
                .source_info()
                .and_then(|info| info.len)
                .is_some_and(|len| len != container.logical_len() as u64)
            {
                return Poll::Ready(Err(AssetError::InvalidSource.into()));
            }
            state.container = Some(container);
        }
        if state.image.is_none() {
            let container = state.container.expect("parsed container");
            state.image = Some(if let Some(image) = container.flat_image() {
                SourceImage {
                    layout: layout(image)?,
                    range: image.main_range(),
                    logical_len: image.logical_len(),
                    encoded: None,
                }
            } else {
                // Retain the same opened object across metadata and value publication.
                if state.source.is_none() {
                    state.source = Some(cx.take_source()?);
                }
                let source = state.source.as_mut().unwrap();
                let mapped = if state.chunk.is_none() {
                    source
                        .mapped()?
                        .map(SourceImage::mapped)
                        .transpose()?
                        .flatten()
                } else {
                    None
                };
                if let Some(image) = mapped {
                    image
                } else {
                    if source.info().ok_or(AssetError::InvalidSource)?.access
                        != crate::core::resource::SourceAccess::Random
                    {
                        return Poll::Ready(Err(AssetError::Unsupported.into()));
                    }
                    if state.chunk.is_none() {
                        let pool = self.pool()?;
                        let allocation = ready!(
                            pool.poll_take(&mut core::task::Context::from_waker(cx.waker()))
                        )
                        .map_err(|_| AssetError::Capacity)?
                        .workspace(core::mem::size_of::<chunk::ChunkRead>())
                        .map_err(|_| AssetError::Capacity)?;
                        state.chunk = Some(Box::new(chunk::ChunkRead::new(container, allocation)?));
                    }
                    let image = ready!(
                        state
                            .chunk
                            .as_mut()
                            .unwrap()
                            .poll(&mut cx.using_source(source), &self.pool()?)
                    )?;
                    state.chunk = None;
                    image
                }
            });
        }
        if state.length_check != LengthCheck::Complete {
            let info = state
                .source
                .as_ref()
                .and_then(AssetSource::info)
                .or_else(|| cx.source_info())
                .ok_or(AssetError::InvalidSource)?;
            if info.len.is_none() {
                if info.access != crate::core::resource::SourceAccess::Random {
                    return Poll::Ready(Err(AssetError::Unsupported.into()));
                }
                let end = state.image.as_ref().expect("probed image").logical_len as u64;
                if let Some(source) = state.source.as_mut() {
                    ready!(state.length_check.poll(end, &mut cx.using_source(source)))?;
                } else {
                    ready!(state.length_check.poll(end, cx))?;
                }
            }
            state.length_check = LengthCheck::Complete;
        }
        let layout = state.image.as_ref().expect("probed image").layout;
        Poll::Ready(Ok(TextureMeta {
            width: layout.width(),
            height: layout.height(),
            format: layout.format(),
        }))
    }
}
