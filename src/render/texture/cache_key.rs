#[cfg(test)]
use super::ColorFormat;
use super::Texture;
use crate::core::threading::Shared;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TextureKey {
    ptr: usize,
    len: usize,
    width: u16,
    height: u16,
    format: mirx::image::ColorFormat,
    palette: usize,
    bit: u8,
    stride: usize,
    revision: u64,
    owner: Option<Shared<()>>,
}

impl TextureKey {
    pub fn cacheable(src: &Texture) -> Option<Self> {
        (!src.transient).then(|| Self::from(src))
    }

    pub fn from(src: &Texture) -> Self {
        let index = src.indexed_plane().map(|(_, index)| index);
        let (ptr, len, owner) = match &src.buf {
            super::TexBuf::Decoded(source) => {
                let owner = source.identity();
                (
                    Shared::as_ptr(&owner) as usize,
                    source.layout().required_bytes(),
                    Some(owner),
                )
            }
            super::TexBuf::Mapped(source) => {
                let owner = source.identity();
                (
                    Shared::as_ptr(&owner) as usize,
                    source.cache_size(),
                    Some(owner),
                )
            }
            super::TexBuf::Native(native) => {
                let owner = native.identity();
                (
                    Shared::as_ptr(&owner) as usize,
                    native.layout().required_bytes(),
                    Some(owner),
                )
            }
            super::TexBuf::Indexed(bytes, _) => (bytes.as_ptr() as usize, bytes.len(), None),
            super::TexBuf::IndexedAligned(source) => (
                source.0.as_slice().as_ptr() as usize,
                source.0.as_slice().len(),
                None,
            ),
            _ => {
                let buf = src.buf.cpu_bytes().expect("CPU storage");
                (buf.as_ptr() as usize, buf.len(), None)
            }
        };
        Self {
            ptr,
            len,
            width: src.width,
            height: src.height,
            format: src.sample_format(),
            palette: index.map_or(0, |index| index.palette.as_bytes().as_ptr() as usize),
            bit: index.map_or(0, |index| index.layout.bit),
            stride: index.map_or(src.stride, |index| index.layout.stride),
            revision: src.cache_revision,
            owner,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/render/texture/cache_key.rs"]
mod tests;
