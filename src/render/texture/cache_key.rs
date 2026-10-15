use super::{ColorFormat, Texture};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TextureKey {
    ptr: usize,
    len: usize,
    width: u16,
    height: u16,
    format: ColorFormat,
    stride: usize,
    revision: u64,
}

impl TextureKey {
    pub fn cacheable(src: &Texture) -> Option<Self> {
        (!src.transient).then(|| Self::from(src))
    }

    pub fn from(src: &Texture) -> Self {
        let buf = src.buf.as_slice();
        Self {
            ptr: buf.as_ptr() as usize,
            len: buf.len(),
            width: src.width,
            height: src.height,
            format: src.format,
            stride: src.stride,
            revision: src.cache_revision,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_collides_when_buffer_view_matches() {
        let buf = [0u8; 16];
        let a = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        let b = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        assert_eq!(TextureKey::from(&a), TextureKey::from(&b));
    }

    #[test]
    fn key_separates_strides_over_one_buffer() {
        let buf = [0u8; 24];
        let a = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        let mut b = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        b.stride = 12;
        assert_ne!(TextureKey::from(&a), TextureKey::from(&b));
    }

    #[test]
    fn key_separates_content_revisions_over_one_buffer() {
        let buf = [0u8; 16];
        let a = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        let b = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888).with_cache_revision(7);
        assert_ne!(TextureKey::from(&a), TextureKey::from(&b));
    }

    #[test]
    fn key_separates_storage_layouts_over_one_buffer() {
        let buf = [0u8; 16];
        let base = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        for other in [
            Texture::from_ref(&buf, 1, 4, ColorFormat::RGBA8888),
            Texture::from_ref(&buf, 2, 2, ColorFormat::BGRA8888),
            Texture::from_ref(&buf[..8], 2, 1, ColorFormat::RGBA8888),
        ] {
            assert_ne!(TextureKey::from(&base), TextureKey::from(&other));
        }
    }

    #[test]
    fn mutable_and_borrowed_storage_requires_explicit_cache_policy() {
        let buf = [0u8; 16];
        let borrowed = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888);
        assert!(TextureKey::cacheable(&borrowed).is_none());

        let revised = Texture::from_ref(&buf, 2, 2, ColorFormat::RGBA8888).with_cache_revision(7);
        assert_eq!(
            TextureKey::cacheable(&revised),
            Some(TextureKey::from(&revised))
        );

        let owned = Texture::owned(2, 2, ColorFormat::RGBA8888);
        assert!(TextureKey::cacheable(&owned).is_none());

        let mut pixels = [0u8; 16];
        let mutable = Texture::new(&mut pixels, 2, 2, ColorFormat::RGBA8888);
        assert!(TextureKey::cacheable(&mutable).is_none());

        static STATIC_PIXELS: [u8; 16] = [0; 16];
        let stable = Texture::from_static(&STATIC_PIXELS, 2, 2, ColorFormat::RGBA8888);
        assert_eq!(
            TextureKey::cacheable(&stable),
            Some(TextureKey::from(&stable))
        );
    }
}
