use crate::format::{StackDescriptor, StackError};
use mirui::render::RenderError;
use mirui::render::texture::{ColorFormat, MirxLoadError, MirxTextureOptions, Texture};

/// Shared image and slice placement for a layered sprite.
///
/// RAW image samples and the placement records borrow the supplied MIRX file.
/// Encoded samples use the regular MIRX texture decoder. Opening the asset
/// validates binary alpha before any frame uses the image.
pub struct SpriteStackAsset {
    descriptor: StackDescriptor<'static>,
    atlas: Texture<'static>,
    file: &'static [u8],
}

/// Reports malformed placement, unsupported atlas data or nonbinary alpha.
#[derive(Debug, PartialEq, Eq)]
pub enum SpriteStackError {
    Placement(StackError),
    Atlas(MirxLoadError),
    Pixels(RenderError),
    PartialAlpha { x: u16, y: u16, alpha: u8 },
}

impl core::fmt::Display for SpriteStackError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Placement(error) => core::fmt::Display::fmt(error, f),
            Self::Atlas(error) => write!(f, "invalid sprite atlas: {error:?}"),
            Self::Pixels(error) => write!(f, "sprite atlas pixel access failed: {error:?}"),
            Self::PartialAlpha { x, y, alpha } => write!(
                f,
                "sprite atlas pixel ({x}, {y}) has alpha {alpha}; expected 0 or 255"
            ),
        }
    }
}

impl core::error::Error for SpriteStackError {}

impl SpriteStackAsset {
    /// Opens a MIRX sprite using embedded image decoding limits.
    pub fn from_mirx(bytes: &'static [u8]) -> Result<Self, SpriteStackError> {
        Self::from_mirx_with(bytes, MirxTextureOptions::new())
    }

    /// Opens a MIRX sprite with explicit texture decoder limits and storage requirements.
    pub fn from_mirx_with(
        bytes: &'static [u8],
        options: MirxTextureOptions,
    ) -> Result<Self, SpriteStackError> {
        let descriptor = StackDescriptor::from_mirx(bytes).map_err(SpriteStackError::Placement)?;
        let atlas = Texture::from_mirx_with(bytes, options).map_err(SpriteStackError::Atlas)?;
        let check_alpha = match atlas.indexed_color_table() {
            Some(table) => table.iter().any(|color| !matches!(color.a, 0 | 255)),
            None => matches!(
                atlas.format(),
                ColorFormat::RGBA8888 | ColorFormat::BGRA8888
            ),
        };
        if check_alpha {
            let pixels = atlas.read_pixels().map_err(SpriteStackError::Pixels)?;
            for y in 0..pixels.height() {
                for x in 0..pixels.width() {
                    let alpha = pixels.get_pixel(i32::from(x), i32::from(y)).a;
                    if alpha != 0 && alpha != 255 {
                        return Err(SpriteStackError::PartialAlpha { x, y, alpha });
                    }
                }
            }
        }
        Ok(Self {
            descriptor,
            atlas,
            file: bytes,
        })
    }

    pub const fn descriptor(&self) -> StackDescriptor<'static> {
        self.descriptor
    }

    /// Borrows the image shared by all slices.
    pub fn atlas(&self) -> &Texture<'static> {
        &self.atlas
    }

    /// The validated descriptor reserves at least one immutable file byte per
    /// layer. Its address distinguishes layers and survives decoded atlas eviction.
    pub(crate) fn layer_revision(&self, index: usize) -> u64 {
        core::ptr::from_ref(&self.file[index]) as usize as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use mirx::Document;
    use mirx::document::EncodeOptions;
    use mirx::image::{
        AtlasMap, ColorDescription, RawImageAsset, Region, SampleLayout, SurfaceDescriptor,
    };

    fn file(pixels: &[u8; 8], with_stack: bool) -> &'static [u8] {
        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
        sprite_file(RawImageAsset::new(surface, &[pixels]), with_stack)
    }

    fn sprite_file(image: RawImageAsset<'_, '_>, with_stack: bool) -> &'static [u8] {
        let mut file = Document::new();
        let atlas = file.push_image(&image).unwrap();
        file.set_primary(atlas).unwrap();
        if with_stack {
            let regions = [
                Region::new(1, 0, 1, 1).unwrap(),
                Region::new(0, 0, 1, 1).unwrap(),
            ];
            let map = AtlasMap::new(2, 1, &regions).unwrap();
            let stack = StackDescriptor::new((1, 1), [0, 0], 256, map).unwrap();
            file.push_extension(stack.extension()).unwrap();
        }
        Box::leak(
            file.encode(&EncodeOptions::new())
                .unwrap()
                .into_boxed_slice(),
        )
    }

    #[test]
    fn only_used_palette_entries_need_binary_alpha() {
        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::I2, ColorDescription::SRGB).unwrap();
        let palette = [[7, 8, 9, 255], [1, 2, 3, 0], [4, 5, 6, 72], [0; 4]];
        let indexed = |byte| {
            sprite_file(
                RawImageAsset::new(surface, &[&[byte]]).with_color_table(palette.as_flattened()),
                true,
            )
        };
        let asset = SpriteStackAsset::from_mirx(indexed(0b00_01_10_00)).unwrap();
        let pixels = asset.atlas().read_pixels().unwrap();
        assert_eq!(pixels.get_pixel(0, 0).a, 255);
        assert_eq!(pixels.get_pixel(1, 0).a, 0);
        assert_eq!(
            SpriteStackAsset::from_mirx(indexed(0b00_10_00_00)).err(),
            Some(SpriteStackError::PartialAlpha {
                x: 1,
                y: 0,
                alpha: 72
            })
        );
    }

    #[test]
    fn sprite_and_standard_image_borrow_the_same_atlas() {
        let bytes = file(&[22, 33, 44, 255, 0, 0, 0, 0], true);
        let asset = SpriteStackAsset::from_mirx(bytes).unwrap();
        let plain = Texture::from_mirx(bytes).unwrap();
        let sprite_pixels = asset.atlas().read_pixels().unwrap();
        assert_eq!(
            sprite_pixels.bytes().as_ptr(),
            plain.read_pixels().unwrap().bytes().as_ptr()
        );
        assert_eq!(asset.descriptor().layers().get(0).unwrap().x(), 1);
        assert_eq!(sprite_pixels.get_pixel(1, 0).a, 0);
        assert_eq!(sprite_pixels.get_pixel(0, 0).a, 255);
    }

    #[test]
    fn missing_metadata_and_partial_alpha_have_specific_load_errors() {
        let bytes = file(&[0, 0, 0, 0, 0, 0, 0, 255], false);
        assert!(Texture::from_mirx(bytes).is_ok());
        assert_eq!(
            SpriteStackAsset::from_mirx(bytes).err(),
            Some(SpriteStackError::Placement(StackError::MissingDescriptor))
        );
        let bytes = file(&[0, 0, 0, 0, 0, 0, 0, 64], true);
        assert_eq!(
            SpriteStackAsset::from_mirx(bytes).err(),
            Some(SpriteStackError::PartialAlpha {
                x: 1,
                y: 0,
                alpha: 64
            })
        );
    }
}
