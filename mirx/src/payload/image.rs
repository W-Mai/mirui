use crate::header::{FLAT_HEADER_LEN, FlatHeader};
use crate::wire::slice;
use crate::{ColorFormat, ReadError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ImageMeta {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: u32,
    pub(crate) format: ColorFormat,
}

/// Borrowed view over validated MIRX image planes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageView<'a> {
    meta: ImageMeta,
    main: &'a [u8],
    extra: Option<&'a [u8]>,
}

impl<'a> ImageView<'a> {
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
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::Reader;
    use crate::crc32;
    use crate::header::{Layout, MAGIC, VERSION_MAJOR, VERSION_MINOR};

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
