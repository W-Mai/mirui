use super::{
    EncodedImageError, EncodedImageView, RawImageView, RawImageViewError, SurfaceDescriptor,
    SurfaceView,
};
use crate::{
    ColorTableView,
    media::{MediaPayload, MediaPayloadError, MediaSectionKind},
};

#[cfg(test)]
mod tests;

/// Borrowed IMAGE storage with explicit sample-access semantics.
///
/// RAW contains verified decoded samples. Encoded contains metadata and requires
/// separate group, integrity and codec checks before decoding. Common metadata
/// does not imply direct sample access or an implicit allocation.
///
/// ```
/// use mirx::image::{ColorDescription, ImageRef, RawImageAsset, SampleLayout, SurfaceDescriptor};
///
/// let surface = SurfaceDescriptor::new(4, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
/// let bytes = RawImageAsset::new(surface, &[&[42; 4]]).encode().unwrap();
/// let image = ImageRef::open(&bytes).unwrap();
/// assert_eq!(image.surface(), surface);
/// assert_eq!(image.raw().unwrap().plane(0).unwrap().bytes(), &[42; 4]);
/// assert!(image.encoded().is_none());
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageRef<'a> {
    Raw(SurfaceView<'a>),
    Encoded(EncodedImageView<'a>),
}

impl<'a> ImageRef<'a> {
    /// Opens an IMAGE payload without assuming its containing file position.
    pub fn open(payload: &'a [u8]) -> Result<Self, ImageReadError> {
        Self::from_payload(payload, None)
    }

    /// Checks RAW file alignment immediately; encoded group preparation uses
    /// the retained file offset for its corresponding alignment checks.
    pub fn open_at(payload: &'a [u8], file_offset: u32) -> Result<Self, ImageReadError> {
        Self::from_payload(payload, Some(file_offset))
    }

    pub const fn surface(self) -> SurfaceDescriptor {
        match self {
            Self::Raw(image) => image.surface(),
            Self::Encoded(image) => image.surface(),
        }
    }

    pub const fn color_table(self) -> Option<ColorTableView<'a>> {
        match self {
            Self::Raw(image) => image.color_table(),
            Self::Encoded(image) => image.color_table(),
        }
    }

    /// Directly borrowed samples, without decoding or converting encoded storage.
    pub const fn raw(self) -> Option<SurfaceView<'a>> {
        match self {
            Self::Raw(image) => Some(image),
            Self::Encoded(_) => None,
        }
    }

    pub const fn encoded(self) -> Option<EncodedImageView<'a>> {
        match self {
            Self::Raw(_) => None,
            Self::Encoded(image) => Some(image),
        }
    }

    fn from_payload(payload: &'a [u8], file_offset: Option<u32>) -> Result<Self, ImageReadError> {
        let media = MediaPayload::open(payload).map_err(ImageReadError::Media)?;
        if media.section(MediaSectionKind::CODINGS).is_some() {
            EncodedImageView::from_media(media, file_offset)
                .map(Self::Encoded)
                .map_err(ImageReadError::Encoded)
        } else {
            RawImageView::from_media(media, file_offset)
                .map(|image| Self::Raw(image.view()))
                .map_err(ImageReadError::Raw)
        }
    }
}

impl<'a> From<SurfaceView<'a>> for ImageRef<'a> {
    fn from(image: SurfaceView<'a>) -> Self {
        Self::Raw(image)
    }
}

impl<'a> From<EncodedImageView<'a>> for ImageRef<'a> {
    fn from(image: EncodedImageView<'a>) -> Self {
        Self::Encoded(image)
    }
}

/// Failure in the common envelope or selected IMAGE storage contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ImageReadError {
    Media(MediaPayloadError),
    Raw(RawImageViewError),
    Encoded(EncodedImageError),
}
