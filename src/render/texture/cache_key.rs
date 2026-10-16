use super::{ColorFormat, Texture};
use alloc::rc::Rc;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TextureKey {
    ptr: usize,
    len: usize,
    width: u16,
    height: u16,
    format: ColorFormat,
    stride: usize,
    revision: u64,
    owner: Option<Rc<()>>,
}

impl TextureKey {
    pub fn cacheable(src: &Texture) -> Option<Self> {
        (!src.transient).then(|| Self::from(src))
    }

    pub fn from(src: &Texture) -> Self {
        let (ptr, len, owner) = match &src.buf {
            super::TexBuf::Native(native) => {
                let owner = native.identity();
                (
                    Rc::as_ptr(&owner) as usize,
                    native.layout().required_bytes(),
                    Some(owner),
                )
            }
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
            format: src.format,
            stride: src.stride,
            revision: src.cache_revision,
            owner,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_collides_when_buffer_view_matches() {
        let buf = [0u8; 16];
        let a = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
        let b = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
        assert_eq!(TextureKey::from(&a), TextureKey::from(&b));
    }

    #[test]
    fn key_separates_strides_over_one_buffer() {
        let buf = [0u8; 24];
        let a = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
        let mut b =
            Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
        b.stride = 12;
        assert_ne!(TextureKey::from(&a), TextureKey::from(&b));
    }

    #[test]
    fn key_separates_content_revisions_over_one_buffer() {
        let buf = [0u8; 16];
        let a = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
        let b = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888)
            .expect("valid pixel storage")
            .with_cache_revision(7);
        assert_ne!(TextureKey::from(&a), TextureKey::from(&b));
    }

    #[test]
    fn key_separates_storage_layouts_over_one_buffer() {
        let buf = [0u8; 16];
        let base =
            Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
        for other in [
            Texture::from_ref(&buf, 1, 4, ColorFormat::RGBA8888).expect("valid pixel storage"),
            Texture::from_ref(&buf, 2, 2, ColorFormat::BGRA8888).expect("valid pixel storage"),
            Texture::from_ref(&buf[..8], 2, 1, ColorFormat::RGBA8888).expect("valid pixel storage"),
        ] {
            assert_ne!(TextureKey::from(&base), TextureKey::from(&other));
        }
    }

    #[test]
    fn mutable_and_borrowed_storage_requires_explicit_cache_policy() {
        let buf = [0u8; 16];
        let borrowed =
            Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
        assert!(TextureKey::cacheable(&borrowed).is_none());

        let revised = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888)
            .expect("valid pixel storage")
            .with_cache_revision(7);
        assert_eq!(
            TextureKey::cacheable(&revised),
            Some(TextureKey::from(&revised))
        );

        let owned = Texture::owned(2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
        assert!(TextureKey::cacheable(&owned).is_none());

        let mut pixels = [0u8; 16];
        let mutable =
            Texture::new(&mut pixels, 2, 2, ColorFormat::RGBA8888).expect("valid pixel storage");
        assert!(TextureKey::cacheable(&mutable).is_none());

        static STATIC_PIXELS: [u8; 16] = [0; 16];
        let stable = Texture::from_static(&STATIC_PIXELS, 2, 2, ColorFormat::RGBA8888)
            .expect("valid pixel storage");
        assert_eq!(
            TextureKey::cacheable(&stable),
            Some(TextureKey::from(&stable))
        );
    }
}
