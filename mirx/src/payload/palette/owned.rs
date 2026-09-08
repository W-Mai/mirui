use alloc::vec::Vec;
use core::{mem, mem::size_of};

use super::{COLOR_LEN, HEADER_LEN, PaletteDecodeError, PaletteView, validate_payload_layout};
use crate::{
    payload::envelope::{VERSION, checked_payload_len, write_crc_trailer},
    reader::PayloadLimits,
    types::Color,
};

/// Editable ordered PALETTE color table.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Palette {
    pub colors: Vec<Color>,
}

/// Failure from a positional in-memory PALETTE mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PaletteMutationError {
    IndexOutOfBounds { index: usize, len: usize },
    AllocationFailed,
}

/// Failure while validating or encoding an editable PALETTE value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PaletteEncodeError {
    InvalidPayload(PaletteDecodeError),
    ColorCountOverflow { actual: usize },
    BufferTooSmall { needed: usize, available: usize },
    AllocationFailed,
}

impl From<PaletteDecodeError> for PaletteEncodeError {
    fn from(value: PaletteDecodeError) -> Self {
        Self::InvalidPayload(value)
    }
}

impl Palette {
    pub const fn new() -> Self {
        Self { colors: Vec::new() }
    }

    pub fn from_colors(colors: Vec<Color>) -> Self {
        Self { colors }
    }

    /// Decodes one complete PALETTE payload with one bounded, fallible color
    /// allocation.
    ///
    /// Structural, resource, exact-end, and decoded-byte checks complete before
    /// CRC validation and the first reserve.
    pub fn decode_with_limits(
        payload: &[u8],
        limits: &PayloadLimits,
    ) -> Result<Self, PaletteDecodeError> {
        decode_payload_with_allocator(payload, limits, &mut CheckedDecodeAllocator)
    }

    pub(crate) fn decode_view_with_limits(
        view: PaletteView<'_>,
        limits: &PayloadLimits,
    ) -> Result<Self, PaletteDecodeError> {
        validate_decoded_budget(view.len(), limits)?;
        decode_view_with_allocator(view, &mut CheckedDecodeAllocator)
    }

    /// Appends a color after reserving its slot fallibly.
    pub fn push(&mut self, color: Color) -> Result<(), PaletteMutationError> {
        self.insert(self.colors.len(), color)
    }

    /// Inserts a color at an exact position, including `len()` for append.
    pub fn insert(&mut self, index: usize, color: Color) -> Result<(), PaletteMutationError> {
        self.insert_with(index, color, |colors| {
            colors
                .try_reserve(1)
                .map_err(|_| PaletteMutationError::AllocationFailed)
        })
    }

    /// Replaces one color and returns the previous value.
    pub fn replace(&mut self, index: usize, color: Color) -> Result<Color, PaletteMutationError> {
        let len = self.colors.len();
        let current = self
            .colors
            .get_mut(index)
            .ok_or(PaletteMutationError::IndexOutOfBounds { index, len })?;
        Ok(mem::replace(current, color))
    }

    /// Removes and returns one color at an exact position.
    pub fn remove(&mut self, index: usize) -> Result<Color, PaletteMutationError> {
        let len = self.colors.len();
        if index >= len {
            return Err(PaletteMutationError::IndexOutOfBounds { index, len });
        }
        Ok(self.colors.remove(index))
    }

    /// Moves one color to its final ordinal without allocating.
    pub fn move_color(&mut self, from: usize, to: usize) -> Result<(), PaletteMutationError> {
        let len = self.colors.len();
        if from >= len {
            return Err(PaletteMutationError::IndexOutOfBounds { index: from, len });
        }
        if to >= len {
            return Err(PaletteMutationError::IndexOutOfBounds { index: to, len });
        }
        if from < to {
            self.colors[from..=to].rotate_left(1);
        } else if to < from {
            self.colors[to..=from].rotate_right(1);
        }
        Ok(())
    }

    /// Returns the exact size of this value's canonical PALETTE payload.
    pub fn encoded_payload_len(&self) -> Result<usize, PaletteEncodeError> {
        Ok(self.payload_plan()?.encoded_len())
    }

    /// Encodes a canonical PALETTE payload into the start of `out`.
    ///
    /// Complete sizing precedes the capacity check. Errors leave all of `out`
    /// unchanged, and success preserves its unused suffix.
    pub fn encode_payload_into(&self, out: &mut [u8]) -> Result<usize, PaletteEncodeError> {
        self.payload_plan()?.copy_payload_into(out)
    }

    /// Allocates and encodes one exact-length canonical PALETTE payload.
    pub fn encode_payload(&self) -> Result<Vec<u8>, PaletteEncodeError> {
        self.payload_plan()?.payload_to_vec()
    }

    pub(crate) fn payload_plan(&self) -> Result<PalettePayloadPlan<'_>, PaletteEncodeError> {
        PalettePayloadPlan::new(self)
    }

    fn insert_with(
        &mut self,
        index: usize,
        color: Color,
        reserve: impl FnOnce(&mut Vec<Color>) -> Result<(), PaletteMutationError>,
    ) -> Result<(), PaletteMutationError> {
        let len = self.colors.len();
        if index > len {
            return Err(PaletteMutationError::IndexOutOfBounds { index, len });
        }
        reserve(&mut self.colors)?;
        self.colors.insert(index, color);
        Ok(())
    }

    #[cfg(test)]
    fn encode_payload_with(
        &self,
        reserve: impl FnOnce(&mut Vec<u8>, usize) -> Result<(), PaletteEncodeError>,
    ) -> Result<Vec<u8>, PaletteEncodeError> {
        self.payload_plan()?.payload_to_vec_with(reserve)
    }
}

trait DecodeAllocator {
    fn reserve_colors(
        &mut self,
        colors: &mut Vec<Color>,
        count: usize,
    ) -> Result<(), PaletteDecodeError>;
}

struct CheckedDecodeAllocator;

impl DecodeAllocator for CheckedDecodeAllocator {
    fn reserve_colors(
        &mut self,
        colors: &mut Vec<Color>,
        count: usize,
    ) -> Result<(), PaletteDecodeError> {
        colors
            .try_reserve_exact(count)
            .map_err(|_| PaletteDecodeError::AllocationFailed)
    }
}

fn decode_payload_with_allocator<A: DecodeAllocator>(
    payload: &[u8],
    limits: &PayloadLimits,
    allocator: &mut A,
) -> Result<Palette, PaletteDecodeError> {
    let layout = validate_payload_layout(payload, limits)?;
    validate_decoded_budget(layout.color_count(), limits)?;
    let view = layout.validate_crc()?;
    decode_view_with_allocator(view, allocator)
}

fn decode_view_with_allocator<A: DecodeAllocator>(
    view: PaletteView<'_>,
    allocator: &mut A,
) -> Result<Palette, PaletteDecodeError> {
    let mut colors = Vec::new();
    allocator.reserve_colors(&mut colors, view.len())?;
    colors.extend(view.colors());
    Ok(Palette { colors })
}

fn validate_decoded_budget(
    color_count: usize,
    limits: &PayloadLimits,
) -> Result<(), PaletteDecodeError> {
    let needed = checked_decoded_bytes(color_count)?;
    let limit = limits.max_decoded_bytes();
    if needed > limit {
        return Err(PaletteDecodeError::DecodedBytesLimitExceeded { needed, limit });
    }
    Ok(())
}

fn checked_decoded_bytes(color_count: usize) -> Result<usize, PaletteDecodeError> {
    color_count
        .checked_mul(size_of::<Color>())
        .ok_or(PaletteDecodeError::SizeOverflow)
}

#[derive(Clone, Copy)]
pub(crate) struct PalettePayloadPlan<'a> {
    palette: &'a Palette,
    color_count: u32,
    covered_len: usize,
    payload_len: usize,
}

impl<'a> PalettePayloadPlan<'a> {
    fn new(palette: &'a Palette) -> Result<Self, PaletteEncodeError> {
        let color_count = checked_color_count(palette.colors.len())?;
        let colors_len = palette
            .colors
            .len()
            .checked_mul(COLOR_LEN)
            .ok_or_else(size_overflow)?;
        let covered_len = HEADER_LEN
            .checked_add(colors_len)
            .ok_or_else(size_overflow)?;
        let payload_len = checked_payload_len(covered_len).map_err(|_| size_overflow())?;
        Ok(Self {
            palette,
            color_count,
            covered_len,
            payload_len,
        })
    }

    pub(crate) const fn encoded_len(self) -> usize {
        self.payload_len
    }

    pub(crate) fn validate_limits(self, limits: &PayloadLimits) -> Result<(), PaletteDecodeError> {
        if self.color_count > limits.max_palette_colors() {
            return Err(PaletteDecodeError::TooManyColors {
                count: self.color_count,
                limit: limits.max_palette_colors(),
            });
        }
        validate_decoded_budget(self.palette.colors.len(), limits)
    }

    pub(crate) fn equals_payload(self, candidate: &[u8]) -> bool {
        if candidate.len() != self.payload_len {
            return false;
        }
        let count = self.color_count.to_le_bytes();
        if candidate[..HEADER_LEN]
            != [
                VERSION,
                crate::ColorFormat::RGBA8888.to_u8(),
                0,
                0,
                count[0],
                count[1],
                count[2],
                count[3],
            ]
        {
            return false;
        }
        let mut offset = HEADER_LEN;
        for color in &self.palette.colors {
            let end = offset + COLOR_LEN;
            if candidate[offset..end] != [color.r, color.g, color.b, color.a] {
                return false;
            }
            offset = end;
        }
        let expected_crc = crate::crc32(&candidate[..self.covered_len]).to_le_bytes();
        candidate[self.covered_len..] == expected_crc
    }

    fn copy_payload_into(self, out: &mut [u8]) -> Result<usize, PaletteEncodeError> {
        if out.len() < self.payload_len {
            return Err(PaletteEncodeError::BufferTooSmall {
                needed: self.payload_len,
                available: out.len(),
            });
        }

        let (payload, _) = out.split_at_mut(self.payload_len);
        payload[0] = VERSION;
        payload[1] = crate::ColorFormat::RGBA8888.to_u8();
        payload[2..4].fill(0);
        payload[4..8].copy_from_slice(&self.color_count.to_le_bytes());

        let mut offset = HEADER_LEN;
        for color in &self.palette.colors {
            payload[offset..offset + COLOR_LEN]
                .copy_from_slice(&[color.r, color.g, color.b, color.a]);
            offset += COLOR_LEN;
        }
        debug_assert_eq!(offset, self.covered_len);
        let (covered, trailer) = payload.split_at_mut(self.covered_len);
        let trailer: &mut [u8; 4] = trailer
            .try_into()
            .expect("planned PALETTE payload has one CRC trailer");
        write_crc_trailer(covered, trailer);
        Ok(self.payload_len)
    }

    pub(crate) fn payload_to_vec(self) -> Result<Vec<u8>, PaletteEncodeError> {
        self.payload_to_vec_with(|bytes, needed| {
            bytes
                .try_reserve_exact(needed)
                .map_err(|_| PaletteEncodeError::AllocationFailed)
        })
    }

    fn payload_to_vec_with(
        self,
        reserve: impl FnOnce(&mut Vec<u8>, usize) -> Result<(), PaletteEncodeError>,
    ) -> Result<Vec<u8>, PaletteEncodeError> {
        let mut payload = Vec::new();
        reserve(&mut payload, self.payload_len)?;
        payload.resize(self.payload_len, 0);
        self.copy_payload_into(&mut payload)?;
        Ok(payload)
    }
}

fn checked_color_count(actual: usize) -> Result<u32, PaletteEncodeError> {
    u32::try_from(actual).map_err(|_| PaletteEncodeError::ColorCountOverflow { actual })
}

const fn size_overflow() -> PaletteEncodeError {
    PaletteEncodeError::InvalidPayload(PaletteDecodeError::SizeOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn sample() -> Palette {
        Palette::from_colors(vec![
            Color::rgba(0x10, 0x20, 0x30, 0x40),
            Color::rgba(0xaa, 0xbb, 0xcc, 0xdd),
            Color::rgba(0x10, 0x20, 0x30, 0x40),
        ])
    }

    struct RejectDecodeReserve;

    impl DecodeAllocator for RejectDecodeReserve {
        fn reserve_colors(
            &mut self,
            _: &mut Vec<Color>,
            _: usize,
        ) -> Result<(), PaletteDecodeError> {
            Err(PaletteDecodeError::AllocationFailed)
        }
    }

    #[test]
    fn owned_decode_and_canonical_encode_preserve_order_and_duplicates() {
        let palette = sample();
        let payload = palette.encode_payload().unwrap();
        assert_eq!(payload.len(), 24);
        assert_eq!(payload[..8], [1, 0x61, 0, 0, 3, 0, 0, 0]);
        assert_eq!(
            &payload[8..20],
            &[
                0x10, 0x20, 0x30, 0x40, 0xaa, 0xbb, 0xcc, 0xdd, 0x10, 0x20, 0x30, 0x40
            ]
        );
        assert_eq!(
            Palette::decode_with_limits(&payload, &PayloadLimits::HOST),
            Ok(palette.clone())
        );
        assert!(palette.payload_plan().unwrap().equals_payload(&payload));

        let empty = Palette::new();
        let payload = empty.encode_payload().unwrap();
        assert_eq!(payload.len(), 12);
        assert_eq!(
            Palette::decode_with_limits(&payload, &PayloadLimits::EMBEDDED),
            Ok(empty)
        );
    }

    #[test]
    fn owned_budget_precedes_crc_and_decode_reserve() {
        let payload = sample().encode_payload().unwrap();
        let limit = size_of::<Color>() * 3 - 1;
        let limits = PayloadLimits::HOST.with_max_decoded_bytes(limit);
        assert_eq!(
            Palette::decode_with_limits(&payload, &limits),
            Err(PaletteDecodeError::DecodedBytesLimitExceeded {
                needed: size_of::<Color>() * 3,
                limit,
            })
        );

        let mut corrupt = payload.clone();
        let last = corrupt.len() - 1;
        corrupt[last] ^= 0x80;
        assert_eq!(
            Palette::decode_with_limits(&corrupt, &limits),
            Err(PaletteDecodeError::DecodedBytesLimitExceeded {
                needed: size_of::<Color>() * 3,
                limit,
            })
        );

        let layout = validate_payload_layout(&payload, &PayloadLimits::HOST).unwrap();
        let view = layout.validate_crc().unwrap();
        assert_eq!(
            decode_view_with_allocator(view, &mut RejectDecodeReserve),
            Err(PaletteDecodeError::AllocationFailed)
        );
    }

    #[test]
    fn positional_crud_and_move_are_exact_and_failure_atomic() {
        let a = Color::rgba(1, 2, 3, 4);
        let b = Color::rgba(5, 6, 7, 8);
        let c = Color::rgba(9, 10, 11, 12);
        let d = Color::rgba(13, 14, 15, 16);
        let mut palette = Palette::from_colors(vec![a, b, c]);

        palette.push(d).unwrap();
        palette.move_color(3, 1).unwrap();
        assert_eq!(palette.colors, [a, d, b, c]);
        assert_eq!(palette.replace(2, a), Ok(b));
        assert_eq!(palette.colors, [a, d, a, c]);
        assert_eq!(palette.remove(1), Ok(d));
        assert_eq!(palette.colors, [a, a, c]);
        palette.move_color(2, 0).unwrap();
        assert_eq!(palette.colors, [c, a, a]);
        palette.move_color(0, 2).unwrap();
        assert_eq!(palette.colors, [a, a, c]);
        palette.move_color(1, 1).unwrap();
        assert_eq!(palette.colors, [a, a, c]);

        let snapshot = palette.clone();
        assert_eq!(
            palette.insert(4, d),
            Err(PaletteMutationError::IndexOutOfBounds { index: 4, len: 3 })
        );
        for error in [
            palette.replace(3, d).unwrap_err(),
            palette.remove(3).unwrap_err(),
            palette.move_color(3, 0).unwrap_err(),
            palette.move_color(0, 3).unwrap_err(),
        ] {
            assert_eq!(
                error,
                PaletteMutationError::IndexOutOfBounds { index: 3, len: 3 }
            );
            assert_eq!(palette, snapshot);
        }

        assert_eq!(
            palette.insert_with(0, d, |_: &mut Vec<Color>| Err(
                PaletteMutationError::AllocationFailed
            )),
            Err(PaletteMutationError::AllocationFailed)
        );
        assert_eq!(palette, snapshot);
    }

    #[test]
    fn encode_into_is_atomic_suffix_safe_and_reserve_fallible() {
        let palette = sample();
        let needed = palette.encoded_payload_len().unwrap();
        let mut short = vec![0xa5; needed - 1];
        let before = short.clone();
        assert_eq!(
            palette.encode_payload_into(&mut short),
            Err(PaletteEncodeError::BufferTooSmall {
                needed,
                available: needed - 1,
            })
        );
        assert_eq!(short, before);

        let mut out = vec![0xa5; needed + 7];
        assert_eq!(palette.encode_payload_into(&mut out), Ok(needed));
        assert_eq!(&out[needed..], &[0xa5; 7]);
        assert_eq!(
            Palette::decode_with_limits(&out[..needed], &PayloadLimits::HOST),
            Ok(palette.clone())
        );

        assert_eq!(
            palette.encode_payload_with(|_, _| Err(PaletteEncodeError::AllocationFailed)),
            Err(PaletteEncodeError::AllocationFailed)
        );
    }

    #[test]
    fn sizing_and_limit_helpers_cover_wire_boundaries() {
        assert_eq!(checked_color_count(u32::MAX as usize), Ok(u32::MAX));
        if usize::BITS > 32 {
            let overflow = u32::MAX as usize + 1;
            assert_eq!(
                checked_color_count(overflow),
                Err(PaletteEncodeError::ColorCountOverflow { actual: overflow })
            );
        }
        assert_eq!(
            checked_decoded_bytes(usize::MAX),
            Err(PaletteDecodeError::SizeOverflow)
        );

        let palette = sample();
        let plan = palette.payload_plan().unwrap();
        let exact = PayloadLimits::HOST
            .with_max_palette_colors(3)
            .with_max_decoded_bytes(size_of::<Color>() * 3);
        assert_eq!(plan.validate_limits(&exact), Ok(()));
        assert_eq!(
            plan.validate_limits(&exact.with_max_palette_colors(2)),
            Err(PaletteDecodeError::TooManyColors { count: 3, limit: 2 })
        );
        assert!(matches!(
            plan.validate_limits(&exact.with_max_decoded_bytes(size_of::<Color>() * 3 - 1)),
            Err(PaletteDecodeError::DecodedBytesLimitExceeded { .. })
        ));

        let payload = palette.encode_payload().unwrap();
        let mut wrong_header = payload.clone();
        wrong_header[2] = 1;
        assert!(!plan.equals_payload(&wrong_header));
        let mut wrong_color = payload.clone();
        wrong_color[8] ^= 1;
        assert!(!plan.equals_payload(&wrong_color));
        let mut wrong_crc = payload.clone();
        *wrong_crc.last_mut().unwrap() ^= 1;
        assert!(!plan.equals_payload(&wrong_crc));
        assert!(!plan.equals_payload(&payload[..payload.len() - 1]));
    }
}
