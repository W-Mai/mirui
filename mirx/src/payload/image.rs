use alloc::{borrow::Cow, vec::Vec};

use super::ColorTableView;
use crate::header::{FLAT_HEADER_LEN, FlatHeader};
use crate::image::{RawImageEncodeError, RawImageView, RawImageViewError, SurfaceView};
use crate::wire::slice;
use crate::{ColorFormat, ReadError};

/// Failure while validating MIRX image metadata, planes, or an IMAGE payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ImagePayloadError {
    Media(RawImageViewError),
    Surface(RawImageEncodeError),
    NotRepresentableAsPacked,
    Truncated { needed: usize, available: usize },
    StrideTooSmall { minimum: u32, actual: u32 },
    MainPlaneLengthMismatch { expected: usize, actual: usize },
    ExtraPlaneLengthMismatch { expected: usize, actual: usize },
    SizeOverflow,
}

/// Failure while encoding a canonical MIRX IMAGE payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ImageEncodeError {
    InvalidPayload(ImagePayloadError),
    BufferTooSmall { needed: usize, available: usize },
    AllocationFailed,
}

impl From<ImagePayloadError> for ImageEncodeError {
    fn from(value: ImagePayloadError) -> Self {
        Self::InvalidPayload(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ImageMeta {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: u32,
    pub(crate) format: ColorFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ImagePlaneSizes {
    pub(crate) main: u32,
    pub(crate) extra: u32,
}

pub(crate) struct ImageAssetParts<'a> {
    pub(crate) meta: ImageMeta,
    pub(crate) main: Cow<'a, [u8]>,
    pub(crate) extra: Option<Cow<'a, [u8]>>,
}

/// Editable MIRX image metadata and copy-on-write planes.
///
/// Construction does not validate plane lengths. Document mutations validate
/// the complete asset before changing document state.
#[derive(Debug, Eq, PartialEq)]
pub struct ImageAsset<'a> {
    meta: ImageMeta,
    main: Cow<'a, [u8]>,
    extra: Option<Cow<'a, [u8]>>,
}

impl<'a> ImageAsset<'a> {
    pub const fn new(
        width: u32,
        height: u32,
        format: ColorFormat,
        stride: u32,
        main: Cow<'a, [u8]>,
    ) -> Self {
        Self {
            meta: ImageMeta {
                width,
                height,
                stride,
                format,
            },
            main,
            extra: None,
        }
    }

    pub fn with_extra(mut self, extra: Cow<'a, [u8]>) -> Self {
        self.extra = Some(extra);
        self
    }

    pub const fn width(&self) -> u32 {
        self.meta.width
    }

    pub const fn height(&self) -> u32 {
        self.meta.height
    }

    pub const fn format(&self) -> ColorFormat {
        self.meta.format
    }

    pub const fn stride(&self) -> u32 {
        self.meta.stride
    }

    pub fn main(&self) -> &[u8] {
        self.main.as_ref()
    }

    pub fn extra(&self) -> Option<&[u8]> {
        self.extra.as_deref()
    }

    /// Returns the exact size of this asset's canonical IMAGE payload.
    pub fn encoded_payload_len(&self) -> Result<usize, ImageEncodeError> {
        Ok(self.payload_plan()?.encoded_len())
    }

    /// Encodes a canonical IMAGE payload into the start of `out`.
    ///
    /// The complete asset is validated before output capacity is inspected.
    /// Errors leave `out` unchanged, and success preserves any unused suffix.
    pub fn encode_payload_into(&self, out: &mut [u8]) -> Result<usize, ImageEncodeError> {
        self.payload_plan()?.copy_payload_into(out)
    }

    /// Allocates and encodes one exact-length canonical IMAGE payload.
    pub fn encode_payload(&self) -> Result<Vec<u8>, ImageEncodeError> {
        self.payload_plan()?.payload_to_vec()
    }

    pub(crate) fn payload_plan(&self) -> Result<ImagePayloadPlan<'_>, ImageEncodeError> {
        let planes = ImagePlanes::new(self.meta, self.main(), self.extra())
            .map_err(ImageEncodeError::InvalidPayload)?;
        ImagePayloadPlan::from_planes(planes).map_err(ImageEncodeError::InvalidPayload)
    }

    pub(crate) const fn meta(&self) -> ImageMeta {
        self.meta
    }

    pub(crate) fn into_parts(self) -> ImageAssetParts<'a> {
        ImageAssetParts {
            meta: self.meta,
            main: self.main,
            extra: self.extra,
        }
    }
}

pub(crate) fn validate_image_planes(
    meta: ImageMeta,
    main: &[u8],
    extra: Option<&[u8]>,
) -> Result<ImagePlaneSizes, ImagePayloadError> {
    let minimum = meta
        .format
        .minimum_stride(meta.width)
        .ok_or(ImagePayloadError::SizeOverflow)?;
    if meta.stride < minimum {
        return Err(ImagePayloadError::StrideTooSmall {
            minimum,
            actual: meta.stride,
        });
    }

    let main_size = meta
        .stride
        .checked_mul(meta.height)
        .ok_or(ImagePayloadError::SizeOverflow)?;
    let extra_size = meta
        .format
        .extra_size(meta.width, meta.height, meta.stride)
        .ok_or(ImagePayloadError::SizeOverflow)?;
    let expected_main = usize::try_from(main_size).map_err(|_| ImagePayloadError::SizeOverflow)?;
    let expected_extra =
        usize::try_from(extra_size).map_err(|_| ImagePayloadError::SizeOverflow)?;

    if main.len() != expected_main {
        return Err(ImagePayloadError::MainPlaneLengthMismatch {
            expected: expected_main,
            actual: main.len(),
        });
    }
    let actual_extra = extra.map_or(0, <[u8]>::len);
    if actual_extra != expected_extra {
        return Err(ImagePayloadError::ExtraPlaneLengthMismatch {
            expected: expected_extra,
            actual: actual_extra,
        });
    }

    Ok(ImagePlaneSizes {
        main: main_size,
        extra: extra_size,
    })
}

#[derive(Clone, Copy)]
pub(crate) struct ImagePlanes<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: ColorFormat,
    pub(crate) stride: u32,
    pub(crate) main: &'a [u8],
    pub(crate) extra: Option<&'a [u8]>,
    pub(crate) main_size: u32,
    pub(crate) extra_size: u32,
}

impl<'a> ImagePlanes<'a> {
    pub(crate) fn new(
        meta: ImageMeta,
        main: &'a [u8],
        extra: Option<&'a [u8]>,
    ) -> Result<Self, ImagePayloadError> {
        let sizes = validate_image_planes(meta, main, extra)?;
        Ok(Self {
            width: meta.width,
            height: meta.height,
            format: meta.format,
            stride: meta.stride,
            main,
            extra,
            main_size: sizes.main,
            extra_size: sizes.extra,
        })
    }

    pub(crate) fn from_view(image: ImageView<'a>) -> Result<Self, ImagePayloadError> {
        let main_size =
            u32::try_from(image.main.len()).map_err(|_| ImagePayloadError::SizeOverflow)?;
        let extra_size = u32::try_from(image.extra.map_or(0, <[u8]>::len))
            .map_err(|_| ImagePayloadError::SizeOverflow)?;
        Ok(Self {
            width: image.meta.width,
            height: image.meta.height,
            format: image.meta.format,
            stride: image.meta.stride,
            main: image.main,
            extra: image.extra,
            main_size,
            extra_size,
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ImagePayloadPlan<'a> {
    planes: ImagePlanes<'a>,
    payload_size: u32,
}

impl<'a> ImagePayloadPlan<'a> {
    pub(crate) fn from_planes(planes: ImagePlanes<'a>) -> Result<Self, ImagePayloadError> {
        let surface = ImageView::from_validated_planes(
            ImageMeta {
                width: planes.width,
                height: planes.height,
                stride: planes.stride,
                format: planes.format,
            },
            planes.main,
            planes.extra,
        )
        .surface()
        .map_err(ImagePayloadError::Surface)?;
        let payload_size = surface.encoded_len().map_err(ImagePayloadError::Surface)?;
        let payload_size =
            u32::try_from(payload_size).map_err(|_| ImagePayloadError::SizeOverflow)?;
        Ok(Self {
            planes,
            payload_size,
        })
    }

    pub(crate) fn surface(self) -> SurfaceView<'a> {
        let planes = self.planes;
        ImageView::from_validated_planes(
            ImageMeta {
                width: planes.width,
                height: planes.height,
                stride: planes.stride,
                format: planes.format,
            },
            planes.main,
            planes.extra,
        )
        .surface()
        .expect("validated packed surface")
    }
    pub(crate) fn encoded_len(self) -> usize {
        self.payload_size as usize
    }
    pub(crate) const fn payload_size(self) -> u32 {
        self.payload_size
    }

    pub(crate) fn copy_payload_into(self, out: &mut [u8]) -> Result<usize, ImageEncodeError> {
        self.surface().encode_into(out).map_err(Into::into)
    }

    pub(crate) fn payload_to_vec(self) -> Result<Vec<u8>, ImageEncodeError> {
        self.surface().encode().map_err(Into::into)
    }

    pub(crate) fn equals_payload(self, candidate: &[u8]) -> bool {
        self.surface().matches_payload(candidate).unwrap_or(false)
    }

    pub(crate) fn emit_payload(self, out: &mut [u8]) {
        self.copy_payload_into(out)
            .expect("validated IMAGE output capacity");
    }
}

impl From<RawImageEncodeError> for ImageEncodeError {
    fn from(error: RawImageEncodeError) -> Self {
        match error {
            RawImageEncodeError::AllocationFailed => Self::AllocationFailed,
            RawImageEncodeError::BufferTooSmall { needed, available } => {
                Self::BufferTooSmall { needed, available }
            }
            error => Self::InvalidPayload(ImagePayloadError::Surface(error)),
        }
    }
}

impl crate::image::ImageSource for ImageAsset<'_> {
    fn view(&self) -> Result<SurfaceView<'_>, ImagePayloadError> {
        let planes = ImagePlanes::new(self.meta, self.main(), self.extra())?;
        Ok(ImagePayloadPlan::from_planes(planes)?.surface())
    }
}

/// Borrowed view over validated MIRX image planes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageView<'a> {
    meta: ImageMeta,
    main: &'a [u8],
    extra: Option<&'a [u8]>,
}

impl<'a> ImageView<'a> {
    pub(crate) const fn from_validated_planes(
        meta: ImageMeta,
        main: &'a [u8],
        extra: Option<&'a [u8]>,
    ) -> Self {
        Self { meta, main, extra }
    }

    pub const fn width(&self) -> u32 {
        self.meta.width
    }

    pub const fn height(&self) -> u32 {
        self.meta.height
    }

    pub const fn stride(&self) -> u32 {
        self.meta.stride
    }

    pub const fn format(&self) -> ColorFormat {
        self.meta.format
    }

    pub const fn main(&self) -> &'a [u8] {
        self.main
    }

    pub const fn extra(&self) -> Option<&'a [u8]> {
        self.extra
    }

    /// Returns the inline RGBA palette for an indexed image.
    ///
    /// I1, I2, I4, and I8 images return their exact borrowed color table.
    /// Formats without an inline palette, including RGB565A8, return `None`.
    pub fn inline_palette(&self) -> Option<ColorTableView<'a>> {
        self.meta.format.palette_entries()?;
        ColorTableView::from_rgba_bytes(self.extra?)
    }

    pub(crate) fn from_flat(
        bytes: &'a [u8],
        header: FlatHeader,
    ) -> Result<(Self, usize), ReadError> {
        let format = ColorFormat::from_u8(header.color_format)
            .ok_or(ReadError::UnknownColorFormat(header.color_format))?;
        let minimum = format
            .minimum_stride(header.width)
            .ok_or(ReadError::SizeOverflow)?;
        if header.stride < minimum {
            return Err(ReadError::StrideTooSmall {
                minimum,
                actual: header.stride,
            });
        }

        let stride = usize::try_from(header.stride).map_err(|_| ReadError::SizeOverflow)?;
        let height = usize::try_from(header.height).map_err(|_| ReadError::SizeOverflow)?;
        let main_len = stride.checked_mul(height).ok_or(ReadError::SizeOverflow)?;
        let extra_len = usize::try_from(
            format
                .extra_size(header.width, header.height, header.stride)
                .ok_or(ReadError::SizeOverflow)?,
        )
        .map_err(|_| ReadError::SizeOverflow)?;
        let main_end = FLAT_HEADER_LEN
            .checked_add(main_len)
            .ok_or(ReadError::SizeOverflow)?;
        let needed = main_end
            .checked_add(extra_len)
            .ok_or(ReadError::SizeOverflow)?;
        if bytes.len() < needed {
            return Err(ReadError::Truncated {
                needed,
                available: bytes.len(),
            });
        }

        let main = slice(bytes, FLAT_HEADER_LEN, main_len).ok_or(ReadError::Truncated {
            needed: main_end,
            available: bytes.len(),
        })?;
        let extra = if extra_len == 0 {
            None
        } else {
            Some(
                slice(bytes, main_end, extra_len).ok_or(ReadError::Truncated {
                    needed,
                    available: bytes.len(),
                })?,
            )
        };

        Ok((
            Self {
                meta: ImageMeta {
                    width: header.width,
                    height: header.height,
                    stride: header.stride,
                    format,
                },
                main,
                extra,
            },
            needed,
        ))
    }

    /// Opens a sectioned RAW IMAGE representable by the packed FLAT model.
    pub fn open_payload(payload: &'a [u8]) -> Result<Self, ImagePayloadError> {
        RawImageView::open(payload)
            .map_err(ImagePayloadError::Media)?
            .packed()
            .ok_or(ImagePayloadError::NotRepresentableAsPacked)
    }

    /// Opens a packed sectioned RAW IMAGE and validates its file placement.
    pub fn open_payload_at(
        payload: &'a [u8],
        payload_offset: u32,
    ) -> Result<Self, ImagePayloadError> {
        RawImageView::open_at(payload, payload_offset)
            .map_err(ImagePayloadError::Media)?
            .packed()
            .ok_or(ImagePayloadError::NotRepresentableAsPacked)
    }
}

#[cfg(test)]
mod tests {
    use alloc::borrow::Cow;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::crc32;
    use crate::header::{Layout, MAGIC, VERSION_MAJOR, VERSION_MINOR};
    use crate::media::{MEDIA_HEADER_LEN, MediaPayload, MediaPayloadError, MediaSectionKind};
    use crate::wire::read_u32_le;
    use crate::{ChunkType, Color, ImageChunkInput, Reader, encode_chunk_image, encode_chunks};

    const FORMATS: [ColorFormat; 16] = [
        ColorFormat::I1,
        ColorFormat::I2,
        ColorFormat::I4,
        ColorFormat::I8,
        ColorFormat::A1,
        ColorFormat::A2,
        ColorFormat::A4,
        ColorFormat::A8,
        ColorFormat::L8,
        ColorFormat::RGB565,
        ColorFormat::RGB565Swapped,
        ColorFormat::RGB565A8,
        ColorFormat::RGB888,
        ColorFormat::XRGB8888,
        ColorFormat::RGBA8888,
        ColorFormat::BGRA8888,
    ];

    fn expected_extra_len(format: ColorFormat) -> usize {
        match format {
            ColorFormat::I1 => 8,
            ColorFormat::I2 => 16,
            ColorFormat::I4 => 64,
            ColorFormat::I8 => 1024,
            ColorFormat::RGB565A8 => 27,
            _ => 0,
        }
    }

    fn flat_file(format: ColorFormat, width: u32, height: u32, stride: u32) -> Vec<u8> {
        let main_len = usize::try_from(stride.checked_mul(height).unwrap()).unwrap();
        let extra_len = usize::try_from(format.extra_size(width, height, stride).unwrap()).unwrap();
        let mut bytes = vec![0; FLAT_HEADER_LEN + main_len + extra_len];
        bytes[..4].copy_from_slice(&MAGIC);
        bytes[4] = VERSION_MAJOR;
        bytes[5] = VERSION_MINOR;
        bytes[6] = Layout::Flat.to_u8();
        bytes[8] = format.to_u8();
        bytes[12..16].copy_from_slice(&width.to_le_bytes());
        bytes[16..20].copy_from_slice(&height.to_le_bytes());
        bytes[20..24].copy_from_slice(&stride.to_le_bytes());
        let checksum = crc32(&bytes[..24]);
        bytes[24..28].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    fn first_image(bytes: &[u8]) -> ImageView<'_> {
        let reader = Reader::open(bytes).unwrap();
        for chunk in reader.chunks() {
            if let Some(image) = chunk.image().unwrap() {
                return image.packed().unwrap();
            }
        }
        panic!("missing IMAGE chunk")
    }

    #[test]
    fn checked_encoder_round_trips_every_format_with_padded_rows() {
        let width = 9;
        let height = 3;
        for format in FORMATS {
            let stride = format.minimum_stride(width).unwrap() + 2;
            let main_len = usize::try_from(stride * height).unwrap();
            let extra_len = expected_extra_len(format);
            assert_eq!(
                usize::try_from(format.extra_size(width, height, stride).unwrap()).unwrap(),
                extra_len,
                "{format:?}"
            );
            let main: Vec<u8> = (0..main_len)
                .map(|index| (index as u8).wrapping_mul(17).wrapping_add(format.to_u8()))
                .collect();
            let extra: Vec<u8> = (0..extra_len)
                .map(|index| (index as u8).wrapping_mul(29).wrapping_add(3))
                .collect();
            let asset = ImageAsset::new(width, height, format, stride, Cow::Borrowed(&main));
            let asset = if extra.is_empty() {
                asset
            } else {
                asset.with_extra(Cow::Borrowed(&extra))
            };
            let needed = asset.encoded_payload_len().unwrap();
            assert_eq!(asset.encoded_payload_len(), Ok(needed), "{format:?}");

            let encoded = asset.encode_payload().unwrap();
            let mut output = vec![0xa5; needed + 3];
            assert_eq!(asset.encode_payload_into(&mut output), Ok(needed));
            assert_eq!(&output[..needed], encoded, "{format:?}");
            assert_eq!(&output[needed..], &[0xa5; 3], "{format:?}");
            let raw = RawImageView::open(&encoded).unwrap();
            assert_eq!(raw.plane(0).unwrap().bytes(), main);
            assert!(raw.media().section(MediaSectionKind::PLANES).is_some());

            let image = ImageView::open_payload(&encoded).unwrap();
            assert_eq!(
                image,
                ImageView::open_payload_at(&encoded, 0).unwrap(),
                "{format:?}"
            );
            assert_eq!(image.width(), width, "{format:?}");
            assert_eq!(image.height(), height, "{format:?}");
            assert_eq!(image.stride(), stride, "{format:?}");
            assert_eq!(image.format(), format);
            assert_eq!(image.main(), main, "{format:?}");
            assert_eq!(
                image.extra(),
                (!extra.is_empty()).then_some(extra.as_slice())
            );
        }

        let empty = ImageAsset::new(0, 0, ColorFormat::A8, 0, Cow::Borrowed(&[]))
            .with_extra(Cow::Borrowed(&[]));
        let encoded = empty.encode_payload().unwrap();
        assert_eq!(encoded.len(), 92);
        assert_eq!(ImageView::open_payload(&encoded).unwrap().extra(), None);
    }

    #[test]
    fn encode_into_validates_before_capacity_and_is_failure_atomic() {
        let valid = ImageAsset::new(2, 1, ColorFormat::RGB565A8, 4, Cow::Borrowed(&[1, 2, 3, 4]))
            .with_extra(Cow::Borrowed(&[5, 6]));
        let needed = valid.encoded_payload_len().unwrap();
        let mut short = vec![0xa5; needed - 1];
        let before = short.clone();
        assert_eq!(
            valid.encode_payload_into(&mut short),
            Err(ImageEncodeError::BufferTooSmall {
                needed,
                available: needed - 1,
            })
        );
        assert_eq!(short, before);

        let bad_main = ImageAsset::new(2, 1, ColorFormat::RGB565A8, 4, Cow::Borrowed(&[1, 2, 3]))
            .with_extra(Cow::Borrowed(&[5, 6]));
        let mut output = [0xa5; 8];
        assert_eq!(
            bad_main.encode_payload_into(&mut output),
            Err(ImageEncodeError::InvalidPayload(
                ImagePayloadError::MainPlaneLengthMismatch {
                    expected: 4,
                    actual: 3,
                }
            ))
        );
        assert_eq!(output, [0xa5; 8]);

        let bad_extra =
            ImageAsset::new(2, 1, ColorFormat::RGB565A8, 4, Cow::Borrowed(&[1, 2, 3, 4]))
                .with_extra(Cow::Borrowed(&[5]));
        assert_eq!(
            bad_extra.encode_payload_into(&mut output),
            Err(ImageEncodeError::InvalidPayload(
                ImagePayloadError::ExtraPlaneLengthMismatch {
                    expected: 2,
                    actual: 1,
                }
            ))
        );
        assert_eq!(output, [0xa5; 8]);

        let bad_stride = ImageAsset::new(2, 1, ColorFormat::RGB565, 3, Cow::Borrowed(&[]));
        assert_eq!(
            bad_stride.encode_payload_into(&mut output),
            Err(ImageEncodeError::InvalidPayload(
                ImagePayloadError::StrideTooSmall {
                    minimum: 4,
                    actual: 3,
                }
            ))
        );
        assert_eq!(output, [0xa5; 8]);
    }

    #[test]
    fn payload_size_arithmetic_checks_wire_boundaries_without_allocating() {
        let geometry_overflow =
            ImageAsset::new(u32::MAX, 1, ColorFormat::RGBA8888, 0, Cow::Borrowed(&[]));
        assert_eq!(
            geometry_overflow.encoded_payload_len(),
            Err(ImageEncodeError::InvalidPayload(
                ImagePayloadError::SizeOverflow
            ))
        );
    }

    #[test]
    fn indexed_images_share_borrowed_ordered_inline_palette_views() {
        for (format, entries) in [
            (ColorFormat::I1, 2usize),
            (ColorFormat::I2, 4),
            (ColorFormat::I4, 16),
            (ColorFormat::I8, 256),
        ] {
            let width = 3;
            let height = 2;
            let stride = format.minimum_stride(width).unwrap() + 1;
            let main = vec![0x5a; usize::try_from(stride * height).unwrap()];
            assert_eq!(format.palette_entries(), Some(entries as u32));
            let rgba: Vec<u8> = (0..entries)
                .flat_map(|index| {
                    let index = index as u8;
                    [
                        index,
                        index.wrapping_mul(3),
                        index.wrapping_add(17),
                        index.wrapping_mul(5),
                    ]
                })
                .collect();

            let asset = ImageAsset::new(width, height, format, stride, Cow::Borrowed(&main))
                .with_extra(Cow::Borrowed(&rgba));
            let payload = asset.encode_payload().unwrap();
            let image = ImageView::open_payload(&payload).unwrap();
            let palette = image.inline_palette().unwrap();

            assert_eq!(palette.len(), entries, "{format:?}");
            assert_eq!(palette.as_bytes(), rgba, "{format:?}");
            assert_eq!(palette.as_bytes().as_ptr(), image.extra().unwrap().as_ptr());
            assert_eq!(
                palette.get(entries - 1),
                Some(Color::rgba(
                    (entries - 1) as u8,
                    ((entries - 1) as u8).wrapping_mul(3),
                    ((entries - 1) as u8).wrapping_add(17),
                    ((entries - 1) as u8).wrapping_mul(5),
                )),
                "{format:?}"
            );
            assert_eq!(palette.iter().len(), entries, "{format:?}");

            let chunked = encode_chunks(&[(ChunkType::IMAGE.raw(), 0, payload.as_slice())]);
            let chunk_image = first_image(&chunked);
            let chunk_palette = chunk_image.inline_palette().unwrap();
            assert_eq!(chunk_palette, palette, "{format:?}");
            assert_eq!(
                chunk_palette.as_bytes().as_ptr(),
                chunk_image.extra().unwrap().as_ptr(),
                "{format:?}"
            );

            let mut flat = flat_file(format, width, height, stride);
            let extra_start = FLAT_HEADER_LEN + main.len();
            flat[FLAT_HEADER_LEN..extra_start].copy_from_slice(&main);
            flat[extra_start..].copy_from_slice(&rgba);
            let flat_image = Reader::open(&flat).unwrap().flat_image().unwrap();
            let flat_palette = flat_image.inline_palette().unwrap();
            assert_eq!(flat_palette.as_bytes(), rgba, "{format:?}");
            assert_eq!(
                flat_palette.as_bytes().as_ptr(),
                flat[extra_start..].as_ptr()
            );

            let zero = ImageAsset::new(0, 0, format, 0, Cow::Borrowed(&[]))
                .with_extra(Cow::Borrowed(&rgba));
            assert_eq!(
                ImageView::open_payload(&zero.encode_payload().unwrap())
                    .unwrap()
                    .inline_palette()
                    .unwrap()
                    .len(),
                entries,
                "{format:?}"
            );
        }

        for format in FORMATS {
            if format.palette_entries().is_some() {
                continue;
            }
            let width = 2;
            let height = 2;
            let stride = format.minimum_stride(width).unwrap();
            let main = vec![0; usize::try_from(stride * height).unwrap()];
            let extra = vec![
                0;
                usize::try_from(format.extra_size(width, height, stride).unwrap())
                    .unwrap()
            ];
            let asset = ImageAsset::new(width, height, format, stride, Cow::Borrowed(&main));
            let asset = if extra.is_empty() {
                asset
            } else {
                asset.with_extra(Cow::Borrowed(extra.as_slice()))
            };
            let payload = asset.encode_payload().unwrap();
            let image = ImageView::open_payload(&payload).unwrap();
            assert_eq!(image.inline_palette(), None, "{format:?}");
            if format == ColorFormat::RGB565A8 {
                assert_eq!(image.extra(), Some(extra.as_slice()));
            }
        }
    }

    #[test]
    fn public_payload_views_borrow_every_format_without_copying() {
        for format in FORMATS {
            let width = 3;
            let height = 2;
            let stride = format.minimum_stride(width).unwrap() + 1;
            let main_len = usize::try_from(stride * height).unwrap();
            let extra_len =
                usize::try_from(format.extra_size(width, height, stride).unwrap()).unwrap();
            let main = vec![format.to_u8(); main_len];
            let extra = vec![format.to_u8() ^ 0xff; extra_len];
            let encoded = encode_chunk_image(&ImageChunkInput {
                width,
                height,
                format,
                stride,
                main: &main,
                extra: if extra.is_empty() {
                    None
                } else {
                    Some(extra.as_slice())
                },
            });
            let reader = Reader::open(&encoded).unwrap();
            let chunk = reader.chunks().next().unwrap();
            let image = chunk.image().unwrap().unwrap().packed().unwrap();
            let direct =
                ImageView::open_payload_at(chunk.payload(), chunk.payload_offset()).unwrap();
            let relative = ImageView::open_payload(chunk.payload()).unwrap();
            let media = MediaPayload::open(chunk.payload()).unwrap();
            let data_offset = media
                .section(MediaSectionKind::DATA)
                .unwrap()
                .descriptor()
                .offset() as usize;

            assert_eq!(image, direct, "{format:?}");
            assert_eq!(image, relative, "{format:?}");
            assert_eq!(image.width(), width, "{format:?}");
            assert_eq!(image.height(), height, "{format:?}");
            assert_eq!(image.stride(), stride, "{format:?}");
            assert_eq!(image.format(), format);
            assert_eq!(image.main(), main, "{format:?}");
            assert_eq!(
                image.main().as_ptr(),
                chunk.payload()[data_offset..].as_ptr(),
                "{format:?}"
            );
            match image.extra() {
                Some(view) => {
                    assert_eq!(view, extra, "{format:?}");
                    assert_eq!(
                        view.as_ptr(),
                        chunk.payload()[if format.palette_entries().is_some() {
                            media
                                .section(MediaSectionKind::COLOR_TABLE)
                                .unwrap()
                                .descriptor()
                                .offset() as usize
                        } else {
                            data_offset + main_len
                        }..]
                            .as_ptr(),
                        "{format:?}"
                    );
                }
                None => assert!(extra.is_empty(), "{format:?}"),
            }
        }
    }

    #[test]
    fn chunk_view_is_type_gated_and_borrows_non_primary_image() {
        let valid = encode_chunk_image(&ImageChunkInput {
            width: 1,
            height: 1,
            format: ColorFormat::A8,
            stride: 1,
            main: &[0x5a],
            extra: None,
        });
        let reader = Reader::open(&valid).unwrap();
        let chunk = reader.chunks().next().unwrap();
        let valid_payload = chunk.payload().to_vec();

        let malformed = [1, 0, 0, 0, 0];
        let mixed = encode_chunks(&[
            (ChunkType::META.raw(), 0, malformed.as_slice()),
            (ChunkType::IMAGE.raw(), 0, valid_payload.as_slice()),
            (ChunkType::IMAGE.raw(), 0, malformed.as_slice()),
        ]);
        let reader = Reader::open(&mixed).unwrap();
        let mut chunks = reader.chunks();
        assert_eq!(chunks.next().unwrap().image(), Ok(None));
        let image_chunk = chunks.next().unwrap();
        let image = first_image(&mixed);
        let data_offset = MediaPayload::open(image_chunk.payload())
            .unwrap()
            .section(MediaSectionKind::DATA)
            .unwrap()
            .descriptor()
            .offset() as usize;
        assert_eq!(image.main(), [0x5a]);
        assert_eq!(
            image.main().as_ptr(),
            mixed[usize::try_from(image_chunk.payload_offset()).unwrap() + data_offset..].as_ptr()
        );
        assert_eq!(
            chunks.next().unwrap().image(),
            Err(RawImageViewError::Media(MediaPayloadError::Truncated {
                needed: MEDIA_HEADER_LEN + 4,
                available: 5
            }))
        );
    }

    #[test]
    fn payload_openers_separate_relative_layout_from_absolute_alignment() {
        let payload = ImageAsset::new(1, 1, ColorFormat::A8, 1, Cow::Borrowed(&[0x56]))
            .encode_payload()
            .unwrap();
        let relative = ImageView::open_payload(&payload).unwrap();
        let placed = ImageView::open_payload_at(&payload, 4).unwrap();
        assert_eq!(relative.main().as_ptr(), placed.main().as_ptr());
        // Tight RAW data has byte alignment; the container adds its own padding.
        assert_eq!(relative, ImageView::open_payload_at(&payload, 3).unwrap());
        assert!(ImageView::open_payload_at(&payload, u32::MAX).is_err());
    }

    #[test]
    fn payload_openers_reject_every_short_header() {
        let mut bytes = [0; MEDIA_HEADER_LEN + 4];
        bytes[0] = 1;
        for available in 0..bytes.len() {
            let expected = Err(ImagePayloadError::Media(RawImageViewError::Media(
                MediaPayloadError::Truncated {
                    needed: if available == 0 { 1 } else { bytes.len() },
                    available,
                },
            )));
            assert_eq!(ImageView::open_payload(&bytes[..available]), expected);
            assert_eq!(
                ImageView::open_payload_at(&bytes[..available], u32::MAX),
                expected
            );
        }
    }

    #[test]
    fn chunk_view_stays_within_its_table_record() {
        let encoded = encode_chunk_image(&ImageChunkInput {
            width: 1,
            height: 1,
            format: ColorFormat::A8,
            stride: 1,
            main: &[0x5a],
            extra: None,
        });
        let reader = Reader::open(&encoded).unwrap();
        let payload = reader.chunks().next().unwrap().payload().to_vec();
        let mut bounded = encode_chunks(&[
            (ChunkType::IMAGE.raw(), 0, payload.as_slice()),
            (0xbeef, 0, b"sentinel"),
        ]);
        let table_offset = usize::try_from(read_u32_le(&bounded, 12).unwrap()).unwrap();
        let declared = read_u32_le(&bounded, table_offset + 8).unwrap();
        bounded[table_offset + 8..table_offset + 12].copy_from_slice(&(declared - 1).to_le_bytes());

        let reader = Reader::open(&bounded).unwrap();
        let mut chunks = reader.chunks();
        assert!(matches!(
            chunks.next().unwrap().image(),
            Err(RawImageViewError::Media(_))
        ));
        assert_eq!(chunks.next().unwrap().payload(), b"sentinel");
    }

    #[test]
    fn borrows_padded_rgb565_main_from_source() {
        let bytes = flat_file(ColorFormat::RGB565, 3, 2, 8);
        let image = Reader::open(&bytes).unwrap().flat_image().unwrap();
        assert_eq!(image.width(), 3);
        assert_eq!(image.height(), 2);
        assert_eq!(image.stride(), 8);
        assert_eq!(image.main().len(), 16);
        assert_eq!(image.main().as_ptr(), bytes[FLAT_HEADER_LEN..].as_ptr());
        assert_eq!(image.extra(), None);
    }

    #[test]
    fn borrows_indexed_palette_after_odd_width_rows() {
        let bytes = flat_file(ColorFormat::I4, 3, 2, 2);
        let image = Reader::open(&bytes).unwrap().flat_image().unwrap();
        assert_eq!(image.main().len(), 4);
        assert_eq!(image.extra().unwrap().len(), 64);
        assert_eq!(
            image.extra().unwrap().as_ptr(),
            bytes[FLAT_HEADER_LEN + 4..].as_ptr()
        );
    }

    #[test]
    fn rgb565a8_uses_padded_main_and_tight_alpha() {
        let bytes = flat_file(ColorFormat::RGB565A8, 3, 2, 8);
        let image = Reader::open(&bytes).unwrap().flat_image().unwrap();
        assert_eq!(image.main().len(), 16);
        assert_eq!(image.extra().unwrap().len(), 6);
    }

    #[test]
    fn rejects_small_stride_unknown_format_and_truncated_planes() {
        let mut small_stride = flat_file(ColorFormat::RGB565, 3, 2, 6);
        small_stride[20..24].copy_from_slice(&5u32.to_le_bytes());
        let checksum = crc32(&small_stride[..24]);
        small_stride[24..28].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            Reader::open(&small_stride),
            Err(ReadError::StrideTooSmall {
                minimum: 6,
                actual: 5,
            })
        );

        let mut unknown = flat_file(ColorFormat::RGB565, 1, 1, 2);
        unknown[8] = 0xff;
        let checksum = crc32(&unknown[..24]);
        unknown[24..28].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(
            Reader::open(&unknown),
            Err(ReadError::UnknownColorFormat(0xff))
        );

        for (format, width, height, stride) in [
            (ColorFormat::RGB565, 2, 2, 4),
            (ColorFormat::I4, 3, 2, 2),
            (ColorFormat::RGB565A8, 3, 2, 8),
        ] {
            let mut truncated = flat_file(format, width, height, stride);
            let needed = truncated.len();
            truncated.pop();
            assert_eq!(
                Reader::open(&truncated),
                Err(ReadError::Truncated {
                    needed,
                    available: needed - 1,
                })
            );
        }
    }

    #[test]
    fn rejects_format_size_overflow_and_chunk_has_no_flat_view() {
        let mut overflow = flat_file(ColorFormat::RGB565, 1, 1, 2);
        overflow[8] = ColorFormat::RGB565A8.to_u8();
        overflow[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        overflow[16..20].copy_from_slice(&2u32.to_le_bytes());
        overflow[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
        let checksum = crc32(&overflow[..24]);
        overflow[24..28].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(Reader::open(&overflow), Err(ReadError::SizeOverflow));

        let mut chunk = vec![0; crate::CHUNK_FILE_HEADER_LEN];
        chunk[..4].copy_from_slice(&MAGIC);
        chunk[4] = VERSION_MAJOR;
        chunk[5] = VERSION_MINOR;
        chunk[6] = Layout::Chunk.to_u8();
        chunk[12..16].copy_from_slice(&(crate::CHUNK_FILE_HEADER_LEN as u32).to_le_bytes());
        chunk[16..20].copy_from_slice(&(crate::CHUNK_FILE_HEADER_LEN as u32).to_le_bytes());
        let checksum = crc32(&chunk[..40]);
        chunk[40..44].copy_from_slice(&checksum.to_le_bytes());
        assert_eq!(Reader::open(&chunk).unwrap().flat_image(), None);
    }
}
