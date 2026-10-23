use core::ops::Range;

use super::ReadError;
use crate::ColorFormat;
use crate::header::{FLAT_HEADER_LEN, FlatHeader};
use crate::payload::image::ImageMeta;

/// Checked FLAT geometry and file-relative plane ranges, without pixel storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlatImageLayout {
    pub(crate) meta: ImageMeta,
    main_end: usize,
    logical_len: usize,
}

impl FlatImageLayout {
    pub(super) fn from_header(header: FlatHeader) -> Result<Self, ReadError> {
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
        let logical_len = main_end
            .checked_add(extra_len)
            .ok_or(ReadError::SizeOverflow)?;
        Ok(Self {
            meta: ImageMeta {
                width: header.width,
                height: header.height,
                stride: header.stride,
                format,
            },
            main_end,
            logical_len,
        })
    }

    pub const fn width(self) -> u32 {
        self.meta.width
    }
    pub const fn height(self) -> u32 {
        self.meta.height
    }
    pub const fn stride(self) -> u32 {
        self.meta.stride
    }
    pub const fn format(self) -> ColorFormat {
        self.meta.format
    }
    pub fn main_range(self) -> Range<usize> {
        FLAT_HEADER_LEN..self.main_end
    }
    pub fn extra_range(self) -> Option<Range<usize>> {
        (self.main_end != self.logical_len).then_some(self.main_end..self.logical_len)
    }
    pub const fn logical_len(self) -> usize {
        self.logical_len
    }
}
