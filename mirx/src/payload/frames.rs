use core::iter::FusedIterator;

mod frame_map;
mod owned;
mod sequence;
mod timing;

pub use frame_map::{FrameCounts, FrameMap, FrameMapAsset, FrameMapError, FrameMapIter};
pub use owned::{
    AnimationFrames, AnimationSettings, AssetFrameIter, AtlasFrames, FramesAsset,
    FramesEncodeError, FramesMutationError,
};
pub use sequence::{
    BlendMode, DisposalMode, FRAME_SEQUENCE_RECORD_LEN, FrameSequence, FrameSequenceError,
};
pub use timing::{
    FRAME_TIMING_HEADER_LEN, FrameTiming, FrameTimingAsset, FrameTimingEncoding, FrameTimingError,
};

use super::envelope::{Envelope, EnvelopeError, ExactEnvelope};
use super::image::{ImageMeta, ImageView};
use crate::{ColorFormat, reader::PayloadLimits, wire::read_u32_le};

const HEADER_LEN: usize = 64;
const FRAME_ENTRY_LEN: usize = 32;
const MAX_FRAME_COUNT: u32 = u16::MAX as u32;
const MAX_TIMESCALE_HZ: u32 = 1_000_000;

/// Semantic mode of one FRAMES payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FramesMode {
    Atlas = 0,
    Animation = 1,
}

impl FramesMode {
    const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Atlas),
            1 => Some(Self::Animation),
            _ => None,
        }
    }
}

/// Failure while validating or opening a FRAMES payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FramesDecodeError {
    Truncated { needed: usize, available: usize },
    UnsupportedVersion(u8),
    UnknownMode(u8),
    UnknownColorFormat(u8),
    UnsupportedCompression(u8),
    UnknownFlags(u32),
    ZeroAtlasWidth,
    ZeroAtlasHeight,
    StrideTooSmall { minimum: u32, actual: u32 },
    InvalidAtlasModeFields,
    ZeroCanvasWidth,
    ZeroCanvasHeight,
    InvalidTimescale(u32),
    ZeroDefaultDuration,
    InvalidFrameCount(u32),
    TooManyFrames { count: u32, limit: u32 },
    InvalidFrameTableOffset { expected: u32, actual: u32 },
    InvalidDataOffset { expected: u32, actual: u32 },
    MainSizeMismatch { expected: u32, actual: u32 },
    ExtraSizeMismatch { expected: u32, actual: u32 },
    StoredSizeMismatch { expected: u32, actual: u32 },
    FrameSourceOutOfBounds { index: u16 },
    FrameTargetOutOfBounds { index: u16 },
    InvalidAtlasFrameFields { index: u16 },
    UnknownFrameFlags { index: u16, flags: u32 },
    PayloadLengthMismatch { expected: usize, actual: usize },
    CrcMismatch { expected: u32, actual: u32 },
    SizeOverflow,
}

/// One decoded FRAMES table record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Frame {
    pub source_x: u32,
    pub source_y: u32,
    pub width: u32,
    pub height: u32,
    pub target_x: u32,
    pub target_y: u32,
    pub duration_ticks: u32,
}

/// Iterator that decodes fixed-size frame records without allocating.
#[derive(Clone, Debug)]
pub struct FrameIter<'a> {
    remaining: &'a [u8],
}

impl Iterator for FrameIter<'_> {
    type Item = Frame;

    fn next(&mut self) -> Option<Self::Item> {
        let (entry, remaining) = self.remaining.split_at_checked(FRAME_ENTRY_LEN)?;
        self.remaining = remaining;
        decode_frame(entry)
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        let offset = match n.checked_mul(FRAME_ENTRY_LEN) {
            Some(offset) => offset,
            None => {
                self.remaining = &[];
                return None;
            }
        };
        if offset >= self.remaining.len() {
            self.remaining = &[];
            return None;
        }
        self.remaining = &self.remaining[offset..];
        self.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.remaining.len() / FRAME_ENTRY_LEN;
        (len, Some(len))
    }
}

impl DoubleEndedIterator for FrameIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let split = self.remaining.len().checked_sub(FRAME_ENTRY_LEN)?;
        let (remaining, entry) = self.remaining.split_at(split);
        self.remaining = remaining;
        decode_frame(entry)
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        if n >= self.len() {
            self.remaining = &[];
            return None;
        }
        let retained = self.remaining.len() - n * FRAME_ENTRY_LEN;
        self.remaining = &self.remaining[..retained];
        self.next_back()
    }
}

impl ExactSizeIterator for FrameIter<'_> {}
impl FusedIterator for FrameIter<'_> {}

/// Borrowed byte windows for one row of one frame rectangle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameRow<'a> {
    main: &'a [u8],
    main_bit_offset: u8,
    alpha: Option<&'a [u8]>,
}

impl<'a> FrameRow<'a> {
    /// Returns the smallest main-plane byte window containing the row.
    pub const fn main(&self) -> &'a [u8] {
        self.main
    }

    /// Returns the leading bit offset within the first main-plane byte.
    pub const fn main_bit_offset(&self) -> u8 {
        self.main_bit_offset
    }

    /// Returns the matching tightly packed A8 row for RGB565A8.
    pub const fn alpha(&self) -> Option<&'a [u8]> {
        self.alpha
    }
}

/// Zero-allocation view over one validated FRAMES payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramesView<'a> {
    mode: FramesMode,
    format: ColorFormat,
    atlas_width: u32,
    atlas_height: u32,
    atlas_stride: u32,
    canvas_width: u32,
    canvas_height: u32,
    frame_count: u16,
    timescale_hz: u32,
    default_duration_ticks: u32,
    play_count: u32,
    frame_table: &'a [u8],
    main: &'a [u8],
    extra: &'a [u8],
}

impl<'a> FramesView<'a> {
    /// Opens and fully validates one FRAMES payload without allocating.
    pub fn open_payload(
        payload: &'a [u8],
        limits: &PayloadLimits,
    ) -> Result<Self, FramesDecodeError> {
        validate_payload_layout(payload, limits)?.validate_crc()
    }

    pub const fn mode(&self) -> FramesMode {
        self.mode
    }

    pub const fn format(&self) -> ColorFormat {
        self.format
    }

    pub const fn atlas_width(&self) -> u32 {
        self.atlas_width
    }

    pub const fn atlas_height(&self) -> u32 {
        self.atlas_height
    }

    pub const fn atlas_stride(&self) -> u32 {
        self.atlas_stride
    }

    pub const fn canvas_width(&self) -> u32 {
        self.canvas_width
    }

    pub const fn canvas_height(&self) -> u32 {
        self.canvas_height
    }

    pub const fn len(&self) -> usize {
        self.frame_count as usize
    }

    pub const fn is_empty(&self) -> bool {
        self.frame_count == 0
    }

    pub const fn timescale_hz(&self) -> u32 {
        self.timescale_hz
    }

    pub const fn default_duration_ticks(&self) -> u32 {
        self.default_duration_ticks
    }

    pub const fn play_count(&self) -> u32 {
        self.play_count
    }

    /// Returns the shared atlas as a borrowed image-plane view.
    pub const fn atlas(&self) -> ImageView<'a> {
        ImageView::from_validated_planes(
            ImageMeta {
                width: self.atlas_width,
                height: self.atlas_height,
                stride: self.atlas_stride,
                format: self.format,
            },
            self.main,
            if self.extra.is_empty() {
                None
            } else {
                Some(self.extra)
            },
        )
    }

    /// Iterates over decoded frame records in table order.
    pub const fn frames(&self) -> FrameIter<'a> {
        FrameIter {
            remaining: self.frame_table,
        }
    }

    /// Decodes one frame record by index.
    pub fn frame(&self, index: usize) -> Option<Frame> {
        let start = index.checked_mul(FRAME_ENTRY_LEN)?;
        let end = start.checked_add(FRAME_ENTRY_LEN)?;
        decode_frame(self.frame_table.get(start..end)?)
    }

    /// Returns the borrowed pixel windows for one row of one frame.
    pub fn frame_row(&self, frame_index: usize, row: u32) -> Option<FrameRow<'a>> {
        let frame = self.frame(frame_index)?;
        if row >= frame.height {
            return None;
        }

        let atlas_y = frame.source_y.checked_add(row)?;
        let row_start = u64::from(atlas_y) * u64::from(self.atlas_stride);
        let bits_per_pixel = u64::from(self.format.bits_per_pixel());
        let start_bit = u64::from(frame.source_x) * bits_per_pixel;
        let start_byte = start_bit / 8;
        let main_bit_offset = u8::try_from(start_bit % 8).ok()?;
        let frame_bits = u64::from(frame.width) * bits_per_pixel;
        let window_bits = u64::from(main_bit_offset) + frame_bits;
        let byte_len = window_bits.checked_add(7)? / 8;
        let main_start = row_start.checked_add(start_byte)?;
        let main_end = main_start.checked_add(byte_len)?;
        let main = self
            .main
            .get(usize::try_from(main_start).ok()?..usize::try_from(main_end).ok()?)?;

        let alpha = if self.format == ColorFormat::RGB565A8 {
            let alpha_row = u64::from(atlas_y) * u64::from(self.atlas_width);
            let alpha_start = alpha_row + u64::from(frame.source_x);
            let alpha_end = alpha_start + u64::from(frame.width);
            Some(
                self.extra
                    .get(usize::try_from(alpha_start).ok()?..usize::try_from(alpha_end).ok()?)?,
            )
        } else {
            None
        };

        Some(FrameRow {
            main,
            main_bit_offset,
            alpha,
        })
    }
}

impl<'a> IntoIterator for FramesView<'a> {
    type Item = Frame;
    type IntoIter = FrameIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.frames()
    }
}

pub(crate) struct ValidatedFramesLayout<'a> {
    exact: ExactEnvelope<'a>,
    view: FramesView<'a>,
}

impl<'a> ValidatedFramesLayout<'a> {
    pub(crate) fn validate_crc(self) -> Result<FramesView<'a>, FramesDecodeError> {
        self.exact.validate_crc().map_err(map_envelope_error)?;
        Ok(self.view)
    }
}

pub(crate) fn validate_payload_layout<'a>(
    payload: &'a [u8],
    limits: &PayloadLimits,
) -> Result<ValidatedFramesLayout<'a>, FramesDecodeError> {
    let envelope = Envelope::open_v1(payload, HEADER_LEN).map_err(map_envelope_error)?;
    let covered = envelope.covered();
    let mode = FramesMode::from_u8(covered[1]).ok_or(FramesDecodeError::UnknownMode(covered[1]))?;
    let format = ColorFormat::from_u8(covered[2])
        .ok_or(FramesDecodeError::UnknownColorFormat(covered[2]))?;
    if covered[3] != 0 {
        return Err(FramesDecodeError::UnsupportedCompression(covered[3]));
    }

    let flags = field(covered, 4)?;
    if flags != 0 {
        return Err(FramesDecodeError::UnknownFlags(flags));
    }
    let atlas_width = field(covered, 8)?;
    if atlas_width == 0 {
        return Err(FramesDecodeError::ZeroAtlasWidth);
    }
    let atlas_height = field(covered, 12)?;
    if atlas_height == 0 {
        return Err(FramesDecodeError::ZeroAtlasHeight);
    }
    let atlas_stride = field(covered, 16)?;
    let minimum = format
        .minimum_stride(atlas_width)
        .ok_or(FramesDecodeError::SizeOverflow)?;
    if atlas_stride < minimum {
        return Err(FramesDecodeError::StrideTooSmall {
            minimum,
            actual: atlas_stride,
        });
    }

    let canvas_width = field(covered, 20)?;
    let canvas_height = field(covered, 24)?;
    let frame_count = field(covered, 28)?;
    if !(1..=MAX_FRAME_COUNT).contains(&frame_count) {
        return Err(FramesDecodeError::InvalidFrameCount(frame_count));
    }
    let limit = limits.max_frame_records();
    if frame_count > limit {
        return Err(FramesDecodeError::TooManyFrames {
            count: frame_count,
            limit,
        });
    }
    let timescale_hz = field(covered, 32)?;
    let default_duration_ticks = field(covered, 36)?;
    let play_count = field(covered, 40)?;
    match mode {
        FramesMode::Atlas => {
            if canvas_width != 0
                || canvas_height != 0
                || timescale_hz != 0
                || default_duration_ticks != 0
                || play_count != 0
            {
                return Err(FramesDecodeError::InvalidAtlasModeFields);
            }
        }
        FramesMode::Animation => {
            if canvas_width == 0 {
                return Err(FramesDecodeError::ZeroCanvasWidth);
            }
            if canvas_height == 0 {
                return Err(FramesDecodeError::ZeroCanvasHeight);
            }
            if !(1..=MAX_TIMESCALE_HZ).contains(&timescale_hz) {
                return Err(FramesDecodeError::InvalidTimescale(timescale_hz));
            }
            if default_duration_ticks == 0 {
                return Err(FramesDecodeError::ZeroDefaultDuration);
            }
        }
    }

    let frame_table_offset = field(covered, 44)?;
    if frame_table_offset != HEADER_LEN as u32 {
        return Err(FramesDecodeError::InvalidFrameTableOffset {
            expected: HEADER_LEN as u32,
            actual: frame_table_offset,
        });
    }
    let table_len = frame_count
        .checked_mul(FRAME_ENTRY_LEN as u32)
        .ok_or(FramesDecodeError::SizeOverflow)?;
    let expected_data_offset = (HEADER_LEN as u32)
        .checked_add(table_len)
        .ok_or(FramesDecodeError::SizeOverflow)?;
    let data_offset = field(covered, 48)?;
    if data_offset != expected_data_offset {
        return Err(FramesDecodeError::InvalidDataOffset {
            expected: expected_data_offset,
            actual: data_offset,
        });
    }

    let expected_main = atlas_stride
        .checked_mul(atlas_height)
        .ok_or(FramesDecodeError::SizeOverflow)?;
    let decoded_main_size = field(covered, 56)?;
    if decoded_main_size != expected_main {
        return Err(FramesDecodeError::MainSizeMismatch {
            expected: expected_main,
            actual: decoded_main_size,
        });
    }
    let expected_extra = format
        .extra_size(atlas_width, atlas_height, atlas_stride)
        .ok_or(FramesDecodeError::SizeOverflow)?;
    let decoded_extra_size = field(covered, 60)?;
    if decoded_extra_size != expected_extra {
        return Err(FramesDecodeError::ExtraSizeMismatch {
            expected: expected_extra,
            actual: decoded_extra_size,
        });
    }
    let expected_stored = expected_main
        .checked_add(expected_extra)
        .ok_or(FramesDecodeError::SizeOverflow)?;
    let stored_data_size = field(covered, 52)?;
    if stored_data_size != expected_stored {
        return Err(FramesDecodeError::StoredSizeMismatch {
            expected: expected_stored,
            actual: stored_data_size,
        });
    }

    let covered_len_u32 = data_offset
        .checked_add(stored_data_size)
        .ok_or(FramesDecodeError::SizeOverflow)?;
    let covered_len =
        usize::try_from(covered_len_u32).map_err(|_| FramesDecodeError::SizeOverflow)?;
    let exact = envelope
        .validate_exact_end(covered_len)
        .map_err(map_envelope_error)?;
    let table_start = HEADER_LEN;
    let table_end = usize::try_from(data_offset).map_err(|_| FramesDecodeError::SizeOverflow)?;
    let main_end = table_end
        .checked_add(usize::try_from(expected_main).map_err(|_| FramesDecodeError::SizeOverflow)?)
        .ok_or(FramesDecodeError::SizeOverflow)?;
    let frame_table = exact
        .covered()
        .get(table_start..table_end)
        .ok_or(FramesDecodeError::SizeOverflow)?;
    let main = exact
        .covered()
        .get(table_end..main_end)
        .ok_or(FramesDecodeError::SizeOverflow)?;
    let extra = exact
        .covered()
        .get(main_end..covered_len)
        .ok_or(FramesDecodeError::SizeOverflow)?;

    validate_frame_table(
        frame_table,
        frame_count,
        mode,
        atlas_width,
        atlas_height,
        canvas_width,
        canvas_height,
    )?;

    Ok(ValidatedFramesLayout {
        exact,
        view: FramesView {
            mode,
            format,
            atlas_width,
            atlas_height,
            atlas_stride,
            canvas_width,
            canvas_height,
            frame_count: u16::try_from(frame_count).expect("validated FRAMES count"),
            timescale_hz,
            default_duration_ticks,
            play_count,
            frame_table,
            main,
            extra,
        },
    })
}

fn validate_frame_table(
    table: &[u8],
    frame_count: u32,
    mode: FramesMode,
    atlas_width: u32,
    atlas_height: u32,
    canvas_width: u32,
    canvas_height: u32,
) -> Result<(), FramesDecodeError> {
    for index in 0..frame_count {
        let start = usize::try_from(index)
            .ok()
            .and_then(|index| index.checked_mul(FRAME_ENTRY_LEN))
            .ok_or(FramesDecodeError::SizeOverflow)?;
        let end = start
            .checked_add(FRAME_ENTRY_LEN)
            .ok_or(FramesDecodeError::SizeOverflow)?;
        let entry = table
            .get(start..end)
            .ok_or(FramesDecodeError::SizeOverflow)?;
        let source_x = field(entry, 0)?;
        let source_y = field(entry, 4)?;
        let width = field(entry, 8)?;
        let height = field(entry, 12)?;
        let target_x = field(entry, 16)?;
        let target_y = field(entry, 20)?;
        let duration_ticks = field(entry, 24)?;
        let flags = field(entry, 28)?;
        let index = u16::try_from(index).expect("validated FRAMES count");

        validate_frame(
            Frame {
                source_x,
                source_y,
                width,
                height,
                target_x,
                target_y,
                duration_ticks,
            },
            index,
            mode,
            atlas_width,
            atlas_height,
            canvas_width,
            canvas_height,
        )?;
        if flags != 0 {
            return Err(FramesDecodeError::UnknownFrameFlags { index, flags });
        }
    }
    Ok(())
}

fn validate_frame(
    frame: Frame,
    index: u16,
    mode: FramesMode,
    atlas_width: u32,
    atlas_height: u32,
    canvas_width: u32,
    canvas_height: u32,
) -> Result<(), FramesDecodeError> {
    if frame.width == 0
        || frame.height == 0
        || exceeds(frame.source_x.checked_add(frame.width), atlas_width)
        || exceeds(frame.source_y.checked_add(frame.height), atlas_height)
    {
        return Err(FramesDecodeError::FrameSourceOutOfBounds { index });
    }
    match mode {
        FramesMode::Atlas => {
            if frame.target_x != 0 || frame.target_y != 0 || frame.duration_ticks != 0 {
                return Err(FramesDecodeError::InvalidAtlasFrameFields { index });
            }
        }
        FramesMode::Animation => {
            if exceeds(frame.target_x.checked_add(frame.width), canvas_width)
                || exceeds(frame.target_y.checked_add(frame.height), canvas_height)
            {
                return Err(FramesDecodeError::FrameTargetOutOfBounds { index });
            }
        }
    }
    Ok(())
}

const fn exceeds(value: Option<u32>, bound: u32) -> bool {
    match value {
        Some(value) => value > bound,
        None => true,
    }
}

fn field(bytes: &[u8], offset: usize) -> Result<u32, FramesDecodeError> {
    read_u32_le(bytes, offset).ok_or(FramesDecodeError::SizeOverflow)
}

fn decode_frame(bytes: &[u8]) -> Option<Frame> {
    Some(Frame {
        source_x: read_u32_le(bytes, 0)?,
        source_y: read_u32_le(bytes, 4)?,
        width: read_u32_le(bytes, 8)?,
        height: read_u32_le(bytes, 12)?,
        target_x: read_u32_le(bytes, 16)?,
        target_y: read_u32_le(bytes, 20)?,
        duration_ticks: read_u32_le(bytes, 24)?,
    })
}

fn map_envelope_error(error: EnvelopeError) -> FramesDecodeError {
    match error {
        EnvelopeError::Truncated { needed, available } => {
            FramesDecodeError::Truncated { needed, available }
        }
        EnvelopeError::UnsupportedVersion(version) => {
            FramesDecodeError::UnsupportedVersion(version)
        }
        EnvelopeError::PayloadLengthMismatch { expected, actual } => {
            FramesDecodeError::PayloadLengthMismatch { expected, actual }
        }
        EnvelopeError::CrcMismatch { expected, actual } => {
            FramesDecodeError::CrcMismatch { expected, actual }
        }
        EnvelopeError::SizeOverflow => FramesDecodeError::SizeOverflow,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::crc32;

    const ATLAS_WIDTH: u32 = 3;
    const ATLAS_HEIGHT: u32 = 2;

    fn write_field(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn payload(mode: FramesMode, format: ColorFormat) -> Vec<u8> {
        payload_with_frames(
            mode,
            format,
            ATLAS_WIDTH,
            ATLAS_HEIGHT,
            format.minimum_stride(ATLAS_WIDTH).unwrap(),
            &[Frame {
                source_x: 0,
                source_y: 0,
                width: ATLAS_WIDTH,
                height: ATLAS_HEIGHT,
                target_x: 0,
                target_y: 0,
                duration_ticks: 0,
            }],
        )
    }

    fn payload_with_frames(
        mode: FramesMode,
        format: ColorFormat,
        atlas_width: u32,
        atlas_height: u32,
        stride: u32,
        frames: &[Frame],
    ) -> Vec<u8> {
        let main_size = stride * atlas_height;
        let extra_size = format
            .extra_size(atlas_width, atlas_height, stride)
            .unwrap();
        let data_offset = HEADER_LEN as u32 + frames.len() as u32 * FRAME_ENTRY_LEN as u32;
        let stored_size = main_size + extra_size;
        let mut bytes = vec![0; data_offset as usize + stored_size as usize];
        bytes[0] = 1;
        bytes[1] = mode as u8;
        bytes[2] = format.to_u8();
        write_field(&mut bytes, 8, atlas_width);
        write_field(&mut bytes, 12, atlas_height);
        write_field(&mut bytes, 16, stride);
        write_field(&mut bytes, 28, frames.len() as u32);
        if mode == FramesMode::Animation {
            write_field(&mut bytes, 20, atlas_width + 1);
            write_field(&mut bytes, 24, atlas_height + 1);
            write_field(&mut bytes, 32, 60);
            write_field(&mut bytes, 36, 1);
        }
        write_field(&mut bytes, 44, HEADER_LEN as u32);
        write_field(&mut bytes, 48, data_offset);
        write_field(&mut bytes, 52, stored_size);
        write_field(&mut bytes, 56, main_size);
        write_field(&mut bytes, 60, extra_size);
        for (index, frame) in frames.iter().enumerate() {
            let start = HEADER_LEN + index * FRAME_ENTRY_LEN;
            write_field(&mut bytes, start, frame.source_x);
            write_field(&mut bytes, start + 4, frame.source_y);
            write_field(&mut bytes, start + 8, frame.width);
            write_field(&mut bytes, start + 12, frame.height);
            write_field(&mut bytes, start + 16, frame.target_x);
            write_field(&mut bytes, start + 20, frame.target_y);
            write_field(&mut bytes, start + 24, frame.duration_ticks);
        }
        let checksum = crc32(&bytes);
        bytes.extend_from_slice(&checksum.to_le_bytes());
        bytes
    }

    fn refresh_crc(bytes: &mut [u8]) {
        let trailer = bytes.len() - 4;
        let checksum = crc32(&bytes[..trailer]);
        bytes[trailer..].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn atlas_headers_accept_every_color_format_and_borrow_planes() {
        let formats = [
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

        for format in formats {
            let bytes = payload(FramesMode::Atlas, format);
            let view = FramesView::open_payload(&bytes, &PayloadLimits::HOST).unwrap();
            let table_start = HEADER_LEN;
            let main_start = HEADER_LEN + FRAME_ENTRY_LEN;
            let main_len = (view.atlas_stride() * view.atlas_height()) as usize;
            assert_eq!(view.mode(), FramesMode::Atlas);
            assert_eq!(view.format(), format);
            assert_eq!(view.len(), 1);
            assert!(!view.is_empty());
            assert_eq!(view.frame_table.as_ptr(), bytes[table_start..].as_ptr());
            assert_eq!(view.main.as_ptr(), bytes[main_start..].as_ptr());
            assert_eq!(view.extra.as_ptr(), bytes[main_start + main_len..].as_ptr());
        }
    }

    #[test]
    fn animation_header_preserves_timing_and_canvas_fields() {
        let mut bytes = payload(FramesMode::Animation, ColorFormat::RGBA8888);
        write_field(&mut bytes, 40, 7);
        write_field(&mut bytes, HEADER_LEN + 16, 1);
        write_field(&mut bytes, HEADER_LEN + 20, 1);
        refresh_crc(&mut bytes);

        let view = FramesView::open_payload(&bytes, &PayloadLimits::HOST).unwrap();
        assert_eq!(view.mode(), FramesMode::Animation);
        assert_eq!(view.canvas_width(), 4);
        assert_eq!(view.canvas_height(), 3);
        assert_eq!(view.timescale_hz(), 60);
        assert_eq!(view.default_duration_ticks(), 1);
        assert_eq!(view.play_count(), 7);
    }

    #[test]
    fn frame_access_and_iteration_decode_wire_records_lazily() {
        let records = [
            Frame {
                source_x: 0,
                source_y: 0,
                width: 1,
                height: 1,
                target_x: 2,
                target_y: 1,
                duration_ticks: 0,
            },
            Frame {
                source_x: 1,
                source_y: 0,
                width: 2,
                height: 1,
                target_x: 1,
                target_y: 2,
                duration_ticks: 3,
            },
            Frame {
                source_x: 0,
                source_y: 1,
                width: 5,
                height: 2,
                target_x: 0,
                target_y: 0,
                duration_ticks: 4,
            },
        ];
        let bytes = payload_with_frames(FramesMode::Animation, ColorFormat::A8, 5, 3, 5, &records);
        let view = FramesView::open_payload(&bytes, &PayloadLimits::HOST).unwrap();
        assert_eq!(view.frame(0), Some(records[0]));
        assert_eq!(view.frame(2), Some(records[2]));
        assert_eq!(view.frame(3), None);
        assert_eq!(view.frame(usize::MAX), None);

        let mut iter = view.frames();
        let mut clone = iter.clone();
        assert_eq!(iter.len(), 3);
        assert_eq!(iter.next(), Some(records[0]));
        assert_eq!(iter.next_back(), Some(records[2]));
        assert_eq!(iter.next(), Some(records[1]));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next_back(), None);
        assert_eq!(clone.nth(1), Some(records[1]));
        assert_eq!(clone.nth_back(0), Some(records[2]));
        assert_eq!(view.into_iter().collect::<Vec<_>>(), records);

        fn assert_fused<I: FusedIterator>(_iter: &I) {}
        assert_fused(&iter);
    }

    #[test]
    fn atlas_view_reuses_image_plane_and_palette_access() {
        let bytes = payload(FramesMode::Atlas, ColorFormat::I4);
        let view = FramesView::open_payload(&bytes, &PayloadLimits::HOST).unwrap();
        let atlas = view.atlas();
        assert_eq!(atlas.width(), ATLAS_WIDTH);
        assert_eq!(atlas.height(), ATLAS_HEIGHT);
        assert_eq!(atlas.stride(), 2);
        assert_eq!(atlas.format(), ColorFormat::I4);
        assert_eq!(atlas.main().as_ptr(), view.main.as_ptr());
        assert_eq!(atlas.extra().unwrap().as_ptr(), view.extra.as_ptr());
        assert_eq!(
            atlas.inline_palette().unwrap().as_bytes().as_ptr(),
            view.extra.as_ptr()
        );

        let rgb = payload(FramesMode::Atlas, ColorFormat::RGB565A8);
        let atlas = FramesView::open_payload(&rgb, &PayloadLimits::HOST)
            .unwrap()
            .atlas();
        assert!(atlas.extra().is_some());
        assert_eq!(atlas.inline_palette(), None);
    }

    #[test]
    fn frame_rows_borrow_minimal_windows_for_every_pixel_depth() {
        let formats = [
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
        let frame = Frame {
            source_x: 1,
            source_y: 1,
            width: 3,
            height: 2,
            target_x: 0,
            target_y: 0,
            duration_ticks: 0,
        };

        for format in formats {
            let stride = format.minimum_stride(5).unwrap() + 2;
            let mut bytes = payload_with_frames(FramesMode::Atlas, format, 5, 3, stride, &[frame]);
            let main_start = HEADER_LEN + FRAME_ENTRY_LEN;
            let main_len = (stride * 3) as usize;
            for (index, byte) in bytes[main_start..main_start + main_len]
                .iter_mut()
                .enumerate()
            {
                *byte = index as u8;
            }
            let extra_start = main_start + main_len;
            let extra_end = bytes.len() - 4;
            for (index, byte) in bytes[extra_start..extra_end].iter_mut().enumerate() {
                *byte = (0x80 + index) as u8;
            }
            refresh_crc(&mut bytes);

            let view = FramesView::open_payload(&bytes, &PayloadLimits::HOST).unwrap();
            let row = view.frame_row(0, 0).unwrap();
            let bits_per_pixel = u32::from(format.bits_per_pixel());
            let start_bit = bits_per_pixel;
            let start_byte = start_bit / 8;
            let bit_offset = (start_bit % 8) as u8;
            let byte_len = (u32::from(bit_offset) + 3 * bits_per_pixel).div_ceil(8);
            let expected_start = (stride + start_byte) as usize;
            assert_eq!(row.main_bit_offset(), bit_offset, "{format:?}");
            assert_eq!(
                row.main(),
                &bytes
                    [main_start + expected_start..main_start + expected_start + byte_len as usize],
                "{format:?}"
            );
            assert_eq!(
                row.main().as_ptr(),
                bytes[main_start + expected_start..].as_ptr(),
                "{format:?}"
            );

            if format == ColorFormat::RGB565A8 {
                assert_eq!(row.alpha(), Some(&bytes[extra_start + 6..extra_start + 9]));
                assert_eq!(
                    view.frame_row(0, 1).unwrap().alpha(),
                    Some(&bytes[extra_start + 11..extra_start + 14])
                );
            } else {
                assert_eq!(row.alpha(), None, "{format:?}");
            }
            assert_eq!(view.frame_row(0, 2), None);
            assert_eq!(view.frame_row(1, 0), None);
        }
    }

    #[test]
    fn short_payloads_are_rejected_before_field_access() {
        assert_eq!(
            FramesView::open_payload(&[], &PayloadLimits::HOST),
            Err(FramesDecodeError::Truncated {
                needed: 1,
                available: 0,
            })
        );
        for available in 1..HEADER_LEN + 4 {
            let mut bytes = vec![0; available];
            bytes[0] = 1;
            assert_eq!(
                FramesView::open_payload(&bytes, &PayloadLimits::HOST),
                Err(FramesDecodeError::Truncated {
                    needed: HEADER_LEN + 4,
                    available,
                })
            );
        }
    }

    #[test]
    fn header_discriminants_and_mode_fields_are_strict() {
        let cases = [
            (1, 2, FramesDecodeError::UnknownMode(2)),
            (2, 0xff, FramesDecodeError::UnknownColorFormat(0xff)),
            (3, 1, FramesDecodeError::UnsupportedCompression(1)),
        ];
        for (offset, value, expected) in cases {
            let mut bytes = payload(FramesMode::Atlas, ColorFormat::A8);
            bytes[offset] = value;
            assert_eq!(
                FramesView::open_payload(&bytes, &PayloadLimits::HOST),
                Err(expected)
            );
        }

        let mut unknown_flags = payload(FramesMode::Atlas, ColorFormat::A8);
        write_field(&mut unknown_flags, 4, 1);
        assert_eq!(
            FramesView::open_payload(&unknown_flags, &PayloadLimits::HOST),
            Err(FramesDecodeError::UnknownFlags(1))
        );

        for (offset, expected) in [
            (20, FramesDecodeError::InvalidAtlasModeFields),
            (24, FramesDecodeError::InvalidAtlasModeFields),
            (32, FramesDecodeError::InvalidAtlasModeFields),
            (36, FramesDecodeError::InvalidAtlasModeFields),
            (40, FramesDecodeError::InvalidAtlasModeFields),
        ] {
            let mut bytes = payload(FramesMode::Atlas, ColorFormat::A8);
            write_field(&mut bytes, offset, 1);
            assert_eq!(
                FramesView::open_payload(&bytes, &PayloadLimits::HOST),
                Err(expected)
            );
        }
    }

    #[test]
    fn dimensions_stride_counts_and_animation_clock_are_bounded() {
        let mutations = [
            (8, 0, FramesDecodeError::ZeroAtlasWidth),
            (12, 0, FramesDecodeError::ZeroAtlasHeight),
            (
                16,
                2,
                FramesDecodeError::StrideTooSmall {
                    minimum: 3,
                    actual: 2,
                },
            ),
            (28, 0, FramesDecodeError::InvalidFrameCount(0)),
            (28, 65_536, FramesDecodeError::InvalidFrameCount(65_536)),
        ];
        for (offset, value, expected) in mutations {
            let mut bytes = payload(FramesMode::Atlas, ColorFormat::A8);
            write_field(&mut bytes, offset, value);
            assert_eq!(
                FramesView::open_payload(&bytes, &PayloadLimits::HOST),
                Err(expected)
            );
        }

        let bytes = payload(FramesMode::Atlas, ColorFormat::A8);
        assert_eq!(
            FramesView::open_payload(&bytes, &PayloadLimits::HOST.with_max_frame_records(0),),
            Err(FramesDecodeError::TooManyFrames { count: 1, limit: 0 })
        );

        for (offset, value, expected) in [
            (20, 0, FramesDecodeError::ZeroCanvasWidth),
            (24, 0, FramesDecodeError::ZeroCanvasHeight),
            (32, 0, FramesDecodeError::InvalidTimescale(0)),
            (
                32,
                1_000_001,
                FramesDecodeError::InvalidTimescale(1_000_001),
            ),
            (36, 0, FramesDecodeError::ZeroDefaultDuration),
        ] {
            let mut bytes = payload(FramesMode::Animation, ColorFormat::A8);
            write_field(&mut bytes, offset, value);
            assert_eq!(
                FramesView::open_payload(&bytes, &PayloadLimits::HOST),
                Err(expected)
            );
        }
    }

    #[test]
    fn derived_offsets_sizes_and_exact_length_are_enforced() {
        let mutations = [
            (
                44,
                63,
                FramesDecodeError::InvalidFrameTableOffset {
                    expected: 64,
                    actual: 63,
                },
            ),
            (
                48,
                95,
                FramesDecodeError::InvalidDataOffset {
                    expected: 96,
                    actual: 95,
                },
            ),
            (
                52,
                5,
                FramesDecodeError::StoredSizeMismatch {
                    expected: 6,
                    actual: 5,
                },
            ),
            (
                56,
                5,
                FramesDecodeError::MainSizeMismatch {
                    expected: 6,
                    actual: 5,
                },
            ),
            (
                60,
                1,
                FramesDecodeError::ExtraSizeMismatch {
                    expected: 0,
                    actual: 1,
                },
            ),
        ];
        for (offset, value, expected) in mutations {
            let mut bytes = payload(FramesMode::Atlas, ColorFormat::A8);
            write_field(&mut bytes, offset, value);
            assert_eq!(
                FramesView::open_payload(&bytes, &PayloadLimits::HOST),
                Err(expected)
            );
        }

        let mut trailing = payload(FramesMode::Atlas, ColorFormat::A8);
        trailing.push(0);
        assert_eq!(
            FramesView::open_payload(&trailing, &PayloadLimits::HOST),
            Err(FramesDecodeError::PayloadLengthMismatch {
                expected: 106,
                actual: 107,
            })
        );
    }

    #[test]
    fn every_frame_rectangle_and_reserved_field_is_validated() {
        let cases = [
            (
                HEADER_LEN + 8,
                0,
                FramesDecodeError::FrameSourceOutOfBounds { index: 0 },
            ),
            (
                HEADER_LEN,
                u32::MAX,
                FramesDecodeError::FrameSourceOutOfBounds { index: 0 },
            ),
            (
                HEADER_LEN + 4,
                1,
                FramesDecodeError::FrameSourceOutOfBounds { index: 0 },
            ),
            (
                HEADER_LEN + 16,
                2,
                FramesDecodeError::FrameTargetOutOfBounds { index: 0 },
            ),
            (
                HEADER_LEN + 20,
                2,
                FramesDecodeError::FrameTargetOutOfBounds { index: 0 },
            ),
            (
                HEADER_LEN + 28,
                1,
                FramesDecodeError::UnknownFrameFlags { index: 0, flags: 1 },
            ),
        ];
        for (offset, value, expected) in cases {
            let mut bytes = payload(FramesMode::Animation, ColorFormat::A8);
            write_field(&mut bytes, offset, value);
            assert_eq!(
                FramesView::open_payload(&bytes, &PayloadLimits::HOST),
                Err(expected)
            );
        }

        for offset in [HEADER_LEN + 16, HEADER_LEN + 20, HEADER_LEN + 24] {
            let mut bytes = payload(FramesMode::Atlas, ColorFormat::A8);
            write_field(&mut bytes, offset, 1);
            assert_eq!(
                FramesView::open_payload(&bytes, &PayloadLimits::HOST),
                Err(FramesDecodeError::InvalidAtlasFrameFields { index: 0 })
            );
        }
    }

    #[test]
    fn crc_is_checked_after_the_complete_layout() {
        let mut bytes = payload(FramesMode::Atlas, ColorFormat::A8);
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;
        let expected = u32::from_le_bytes(bytes[last - 3..=last].try_into().unwrap());
        let actual = crc32(&bytes[..last - 3]);
        assert_eq!(
            FramesView::open_payload(&bytes, &PayloadLimits::HOST),
            Err(FramesDecodeError::CrcMismatch { expected, actual })
        );
    }
}
