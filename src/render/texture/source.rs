use alloc::boxed::Box;
#[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
use alloc::rc::Rc;
use core::ops::Range;
use core::task::{Poll, ready};

use super::{TexBuf, Texture, TextureMeta, map_mirx_format};
use crate::core::resource::{
    AssetError, AssetSource, FormatMatch, LoadContext, LoadError, LoadInput, Loader, ProbeLoader,
};
use crate::render::{PlaneLayout, PlaneRequirements};
use mirx::reader::{ContainerLayout, FlatImageLayout, ReadOptions};

pub(super) struct MappedTexture {
    source: AssetSource,
    range: Range<usize>,
    bytes: usize,
    #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
    identity: Rc<()>,
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
    pub(super) fn identity(&self) -> Rc<()> {
        Rc::clone(&self.identity)
    }

    fn into_texture(
        source: AssetSource,
        image: FlatImageLayout,
    ) -> Result<Texture<'static>, LoadError> {
        let layout = layout(image)?;
        let bytes = source.mapped()?.ok_or(AssetError::Unsupported)?.len();
        if bytes != image.logical_len() {
            return Err(AssetError::InvalidSource.into());
        }
        let range = image.main_range();
        if range.len() < layout.minimum_bytes() || range.end > bytes {
            return Err(AssetError::Truncated.into());
        }
        Ok(Texture::from_layout(
            TexBuf::Mapped(Box::new(Self {
                source,
                range,
                bytes,
                #[cfg(any(feature = "sdl-gpu", feature = "wgpu", target_arch = "wasm32"))]
                identity: Rc::new(()),
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
        struct Mapping(Rc<alloc::vec::Vec<u8>>);
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
        let bytes = Rc::new(bytes);
        let mut app: App<FramebufSurface<fn(&[u8], PhysicalRect)>> = App::headless(1, 1);
        app.add_plugin(
            ImageResourcesPlugin::empty().with_source("mapped", |_| Some("same".into())),
        );
        app.add_plugin(
            ResourcePlugin::new()
                .with_source("mapped", move |_| ready(Ok(Mapping(Rc::clone(&bytes))))),
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

pub(crate) struct SourceMirxLoader;

#[derive(Default)]
pub(crate) struct SourceMirxState {
    header: [u8; 28],
    header_len: Option<usize>,
    image: Option<FlatImageLayout>,
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
        Poll::Ready(MappedTexture::into_texture(
            cx.take_source()?,
            state.image.expect("probed image"),
        ))
    }
}

impl ProbeLoader<Texture<'static>> for SourceMirxLoader {
    fn poll_probe(
        &self,
        state: &mut Self::State,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<TextureMeta, LoadError>> {
        if state.image.is_none() {
            if state.header_len.is_none() {
                ready!(cx.poll_read_exact(0, &mut state.header[..8]))?;
                let len = ContainerLayout::header_len(&state.header[..8])
                    .map_err(|_| LoadError::Failed("invalid MIRX header"))?;
                if len != state.header.len() {
                    return Poll::Ready(Err(AssetError::Unsupported.into()));
                }
                state.header_len = Some(len);
            }
            ready!(cx.poll_read_exact(8, &mut state.header[8..]))?;
            let container = ContainerLayout::parse(&state.header, &ReadOptions::default())
                .map_err(|_| LoadError::Failed("invalid MIRX container"))?;
            let image = container.flat_image().ok_or(AssetError::Unsupported)?;
            if cx
                .source_info()
                .and_then(|info| info.len)
                .is_some_and(|len| len != image.logical_len() as u64)
            {
                return Poll::Ready(Err(AssetError::InvalidSource.into()));
            }
            // The immutable descriptor contains no borrowed header or pixel storage.
            layout(image)?;
            state.image = Some(image);
        }
        let layout = layout(state.image.expect("probed image"))?;
        Poll::Ready(Ok(TextureMeta {
            width: layout.width(),
            height: layout.height(),
            format: layout.format(),
        }))
    }
}
