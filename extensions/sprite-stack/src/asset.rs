use crate::format::lod::{LodError, StackLods};
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
    // Indexed levels retain the source palette even if the requested base texture was converted.
    pub(crate) levels: Option<(StackLods<'static>, &'static [u8])>,
}

/// Reports malformed placement, unsupported atlas data or nonbinary alpha.
#[derive(Debug, PartialEq, Eq)]
pub enum SpriteStackError {
    Placement(StackError),
    Levels(LodError),
    Atlas(MirxLoadError),
    Pixels(RenderError),
    PartialAlpha { x: u16, y: u16, alpha: u8 },
}

impl core::fmt::Display for SpriteStackError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Placement(error) => core::fmt::Display::fmt(error, f),
            Self::Levels(error) => core::fmt::Display::fmt(error, f),
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

/// Interprets an open MIRX container as a sprite stack using its optional placement descriptor.
pub trait SpriteStackReaderExt {
    fn sprite_stack(&self) -> Result<SpriteStackAsset, SpriteStackError> {
        self.sprite_stack_with(MirxTextureOptions::new())
    }

    fn sprite_stack_with(
        &self,
        options: MirxTextureOptions,
    ) -> Result<SpriteStackAsset, SpriteStackError>;
}

impl SpriteStackReaderExt for mirx::Reader<'static> {
    fn sprite_stack_with(
        &self,
        options: MirxTextureOptions,
    ) -> Result<SpriteStackAsset, SpriteStackError> {
        let (descriptor, image) =
            StackDescriptor::from_reader_with_atlas(self).map_err(SpriteStackError::Placement)?;
        let levels = StackLods::from_reader(self, descriptor, image)
            .map_err(SpriteStackError::Levels)?
            .map(|levels| {
                (
                    levels,
                    image
                        .color_table()
                        .map_or(&[][..], |table| table.as_bytes()),
                )
            });
        let atlas = Texture::from_mirx_image(image, options).map_err(SpriteStackError::Atlas)?;
        SpriteStackAsset::from_atlas(descriptor, atlas, self.source(), levels)
    }
}

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
        mirx::Reader::open(bytes)
            .map_err(|err| SpriteStackError::Placement(StackError::Container(err)))?
            .sprite_stack_with(options)
    }

    fn from_atlas(
        descriptor: StackDescriptor<'static>,
        atlas: Texture<'static>,
        bytes: &'static [u8],
        levels: Option<(StackLods<'static>, &'static [u8])>,
    ) -> Result<Self, SpriteStackError> {
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
            levels,
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
#[path = "../tests/unit/asset.rs"]
mod tests;
