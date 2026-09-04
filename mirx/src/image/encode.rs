use alloc::vec::Vec;

use super::{
    PLANE_RECORD_LEN, PlaneMemoryError, PlaneMemoryLayout, SURFACE_RECORD_LEN, SurfaceDescriptor,
};
use crate::crc32;
use crate::media::{
    CodingId, MEDIA_CRC_LEN, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION, MediaSectionFlags,
    MediaSectionKind,
};
use crate::wire::{write_u16_le, write_u32_le};

/// Borrowed RAW IMAGE authoring input.
///
/// The outer plane slice and every plane byte slice remain caller-owned. Tight
/// canonical layouts need only those bytes; padded or aligned surfaces attach
/// one checked [`PlaneMemoryLayout`] per derived plane.
#[derive(Clone, Copy, Debug)]
pub struct RawImageAsset<'planes, 'data> {
    surface: SurfaceDescriptor,
    planes: &'planes [&'data [u8]],
    memory: Option<&'planes [PlaneMemoryLayout]>,
    color_table: Option<&'data [u8]>,
}

impl<'planes, 'data> RawImageAsset<'planes, 'data> {
    pub const fn new(surface: SurfaceDescriptor, planes: &'planes [&'data [u8]]) -> Self {
        Self {
            surface,
            planes,
            memory: None,
            color_table: None,
        }
    }

    pub const fn with_memory_layouts(mut self, memory: &'planes [PlaneMemoryLayout]) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Attaches straight-alpha RGBA entries for an indexed sample layout.
    pub const fn with_color_table(mut self, rgba: &'data [u8]) -> Self {
        self.color_table = Some(rgba);
        self
    }

    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }

    pub const fn planes(self) -> &'planes [&'data [u8]] {
        self.planes
    }

    pub const fn memory_layouts(self) -> Option<&'planes [PlaneMemoryLayout]> {
        self.memory
    }

    pub const fn color_table(self) -> Option<&'data [u8]> {
        self.color_table
    }

    /// Validates and borrows the decoded surface without encoding it.
    /// The result borrows plane bytes, not the temporary plane-reference array.
    pub fn view(self) -> Result<super::SurfaceView<'data>, RawImageEncodeError> {
        RawImagePlan::new(self)?;
        Ok(super::SurfaceView::from_asset(self))
    }

    /// Returns the exact canonical payload length after complete validation.
    pub fn encoded_len(self) -> Result<usize, RawImageEncodeError> {
        Ok(RawImagePlan::new(self)?.payload_len)
    }

    /// Encodes into the start of `out` without touching an unused suffix.
    /// Validation and capacity checks complete before the first output byte is
    /// changed.
    pub fn encode_into(self, out: &mut [u8]) -> Result<usize, RawImageEncodeError> {
        RawImagePlan::new(self)?.encode_into(out)
    }

    /// Allocates one exact-length canonical payload.
    pub fn encode(self) -> Result<Vec<u8>, RawImageEncodeError> {
        let plan = RawImagePlan::new(self)?;
        let mut out = Vec::new();
        out.try_reserve_exact(plan.payload_len)
            .map_err(|_| RawImageEncodeError::AllocationFailed)?;
        out.resize(plan.payload_len, 0);
        plan.encode_into(&mut out)?;
        Ok(out)
    }
}

#[derive(Clone, Copy, Debug)]
struct RawImagePlan<'planes, 'data> {
    asset: RawImageAsset<'planes, 'data>,
    section_count: u16,
    planes_offset: Option<usize>,
    color_table_offset: Option<usize>,
    data_offset: usize,
    data_len: u32,
    required_alignment_log2: u8,
    payload_len: usize,
}

impl<'planes, 'data> RawImagePlan<'planes, 'data> {
    fn new(asset: RawImageAsset<'planes, 'data>) -> Result<Self, RawImageEncodeError> {
        let plane_count = usize::from(asset.surface.plane_count());
        if asset.planes.len() != plane_count {
            return Err(RawImageEncodeError::PlaneCountMismatch {
                expected: plane_count,
                actual: asset.planes.len(),
            });
        }
        if let Some(memory) = asset.memory {
            if memory.len() != plane_count {
                return Err(RawImageEncodeError::MemoryLayoutCountMismatch {
                    expected: plane_count,
                    actual: memory.len(),
                });
            }
        }

        let color_table_len = validate_color_table(asset)?;
        let (data_len, required_alignment_log2) = validate_planes(asset)?;
        let has_planes = asset.memory.is_some();
        let section_count = 2usize
            .checked_add(usize::from(has_planes))
            .and_then(|count| count.checked_add(usize::from(color_table_len != 0)))
            .ok_or(RawImageEncodeError::SizeOverflow)?;
        let directory_len = section_count
            .checked_mul(MEDIA_SECTION_LEN)
            .ok_or(RawImageEncodeError::SizeOverflow)?;
        let surface_offset = MEDIA_HEADER_LEN
            .checked_add(directory_len)
            .ok_or(RawImageEncodeError::SizeOverflow)?;
        let mut cursor = surface_offset
            .checked_add(SURFACE_RECORD_LEN)
            .ok_or(RawImageEncodeError::SizeOverflow)?;
        let planes_offset = if has_planes {
            let offset = cursor;
            cursor = cursor
                .checked_add(
                    plane_count
                        .checked_mul(PLANE_RECORD_LEN)
                        .ok_or(RawImageEncodeError::SizeOverflow)?,
                )
                .ok_or(RawImageEncodeError::SizeOverflow)?;
            Some(offset)
        } else {
            None
        };
        let color_table_offset = if color_table_len != 0 {
            let offset = cursor;
            cursor = cursor
                .checked_add(color_table_len)
                .ok_or(RawImageEncodeError::SizeOverflow)?;
            Some(offset)
        } else {
            None
        };
        let alignment = 1usize << required_alignment_log2;
        let data_offset = align_up(cursor, alignment).ok_or(RawImageEncodeError::SizeOverflow)?;
        let payload_len = data_offset
            .checked_add(usize::try_from(data_len).map_err(|_| RawImageEncodeError::SizeOverflow)?)
            .and_then(|end| end.checked_add(MEDIA_CRC_LEN))
            .ok_or(RawImageEncodeError::SizeOverflow)?;
        u32::try_from(payload_len).map_err(|_| RawImageEncodeError::SizeOverflow)?;

        Ok(Self {
            asset,
            section_count: u16::try_from(section_count)
                .map_err(|_| RawImageEncodeError::SizeOverflow)?,
            planes_offset,
            color_table_offset,
            data_offset,
            data_len,
            required_alignment_log2,
            payload_len,
        })
    }

    fn encode_into(self, out: &mut [u8]) -> Result<usize, RawImageEncodeError> {
        if out.len() < self.payload_len {
            return Err(RawImageEncodeError::BufferTooSmall {
                needed: self.payload_len,
                available: out.len(),
            });
        }

        let target = &mut out[..self.payload_len];
        target.fill(0);
        target[0] = MEDIA_VERSION;
        write_u16_le(target, 2, self.section_count);
        write_u16_le(target, 4, MEDIA_SECTION_LEN as u16);
        target[6] = self.required_alignment_log2;
        write_u32_le(target, 8, MEDIA_HEADER_LEN as u32);
        write_u32_le(target, 12, self.payload_len as u32);
        write_u32_le(target, 16, self.data_len);
        write_u16_le(target, 24, CodingId::RAW.raw());

        let surface_offset = MEDIA_HEADER_LEN + usize::from(self.section_count) * MEDIA_SECTION_LEN;
        self.asset
            .surface
            .encode_record_into(&mut target[surface_offset..surface_offset + SURFACE_RECORD_LEN])
            .expect("validated exact surface record output");

        let mut entry_index = 0;
        write_section_entry(
            target,
            entry_index,
            MediaSectionKind::SURFACE,
            surface_offset,
            SURFACE_RECORD_LEN,
        );
        entry_index += 1;

        if let (Some(offset), Some(memory)) = (self.planes_offset, self.asset.memory) {
            for (index, layout) in memory.iter().enumerate() {
                let start = offset + index * PLANE_RECORD_LEN;
                layout
                    .encode_record_into(&mut target[start..start + PLANE_RECORD_LEN])
                    .expect("validated exact plane record output");
            }
            write_section_entry(
                target,
                entry_index,
                MediaSectionKind::PLANES,
                offset,
                memory.len() * PLANE_RECORD_LEN,
            );
            entry_index += 1;
        }

        if let (Some(offset), Some(color_table)) = (self.color_table_offset, self.asset.color_table)
        {
            target[offset..offset + color_table.len()].copy_from_slice(color_table);
            write_section_entry(
                target,
                entry_index,
                MediaSectionKind::COLOR_TABLE,
                offset,
                color_table.len(),
            );
            entry_index += 1;
        }

        write_section_entry(
            target,
            entry_index,
            MediaSectionKind::DATA,
            self.data_offset,
            self.data_len as usize,
        );
        if let Some(memory) = self.asset.memory {
            for (layout, bytes) in memory.iter().zip(self.asset.planes.iter()) {
                let start = self.data_offset + layout.data_offset() as usize;
                target[start..start + bytes.len()].copy_from_slice(bytes);
            }
        } else {
            let mut cursor = self.data_offset;
            for bytes in self.asset.planes {
                target[cursor..cursor + bytes.len()].copy_from_slice(bytes);
                cursor += bytes.len();
            }
        }

        let crc_offset = self.payload_len - MEDIA_CRC_LEN;
        let crc = crc32::compute(&target[..crc_offset]);
        target[crc_offset..].copy_from_slice(&crc.to_le_bytes());
        Ok(self.payload_len)
    }
}

/// Failure while validating or encoding a RAW IMAGE asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RawImageEncodeError {
    PlaneCountMismatch {
        expected: usize,
        actual: usize,
    },
    MemoryLayoutCountMismatch {
        expected: usize,
        actual: usize,
    },
    InvalidPlaneLayout {
        index: u8,
        error: PlaneMemoryError,
    },
    PlaneLengthMismatch {
        index: u8,
        expected: usize,
        actual: usize,
    },
    PlaneRangesOverlap {
        previous: u8,
        next: u8,
    },
    MissingColorTable,
    UnexpectedColorTable,
    ColorTableLengthMismatch {
        expected: usize,
        actual: usize,
    },
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    SizeOverflow,
    AllocationFailed,
}

fn validate_color_table(asset: RawImageAsset<'_, '_>) -> Result<usize, RawImageEncodeError> {
    match (
        asset.surface.sample_layout().color_table_entries(),
        asset.color_table,
    ) {
        (Some(entries), Some(table)) => {
            let expected = usize::try_from(entries)
                .ok()
                .and_then(|entries| entries.checked_mul(4))
                .ok_or(RawImageEncodeError::SizeOverflow)?;
            if table.len() != expected {
                return Err(RawImageEncodeError::ColorTableLengthMismatch {
                    expected,
                    actual: table.len(),
                });
            }
            Ok(expected)
        }
        (Some(_), None) => Err(RawImageEncodeError::MissingColorTable),
        (None, Some(_)) => Err(RawImageEncodeError::UnexpectedColorTable),
        (None, None) => Ok(0),
    }
}

fn validate_planes(asset: RawImageAsset<'_, '_>) -> Result<(u32, u8), RawImageEncodeError> {
    let mut previous_end = 0;
    let mut alignment_log2 = 0;
    for (index, bytes) in asset.planes.iter().enumerate() {
        let index = u8::try_from(index).map_err(|_| RawImageEncodeError::SizeOverflow)?;
        let geometry = asset
            .surface
            .plane(index)
            .ok_or(RawImageEncodeError::SizeOverflow)?;
        let memory = match asset.memory {
            Some(memory) => {
                let layout = memory[usize::from(index)];
                layout
                    .validate_for(geometry)
                    .map_err(|error| RawImageEncodeError::InvalidPlaneLayout { index, error })?;
                layout
            }
            None => PlaneMemoryLayout::builder(geometry)
                .with_data_offset(previous_end)
                .build()
                .map_err(|error| RawImageEncodeError::InvalidPlaneLayout { index, error })?,
        };

        let expected =
            usize::try_from(memory.byte_len()).map_err(|_| RawImageEncodeError::SizeOverflow)?;
        if bytes.len() != expected {
            return Err(RawImageEncodeError::PlaneLengthMismatch {
                index,
                expected,
                actual: bytes.len(),
            });
        }
        if memory.data_offset() < previous_end {
            return Err(RawImageEncodeError::PlaneRangesOverlap {
                previous: index - 1,
                next: index,
            });
        }
        previous_end = memory.data_end();
        alignment_log2 = alignment_log2.max(memory.required_alignment().trailing_zeros() as u8);
    }
    Ok((previous_end, alignment_log2))
}

fn write_section_entry(
    out: &mut [u8],
    index: usize,
    kind: MediaSectionKind,
    offset: usize,
    size: usize,
) {
    let entry = MEDIA_HEADER_LEN + index * MEDIA_SECTION_LEN;
    write_u16_le(out, entry, kind.raw());
    write_u16_le(out, entry + 2, MediaSectionFlags::REQUIRED.bits());
    write_u32_le(out, entry + 4, offset as u32);
    write_u32_le(out, entry + 8, size as u32);
    write_u32_le(out, entry + 12, size as u32);
}

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    value
        .checked_add(alignment.checked_sub(1)?)
        .map(|value| value & !(alignment - 1))
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::image::{ColorDescription, RawImageView, SampleLayout};

    #[test]
    fn tight_nv12_round_trips_without_a_planes_section() {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let y = [0x10; 15];
        let uv = [0x80; 12];
        let planes: &[&[u8]] = &[&y, &uv];
        let asset = RawImageAsset::new(surface, planes);
        let bytes = asset.encode().unwrap();
        assert_eq!(asset.encoded_len(), Ok(bytes.len()));

        let image = RawImageView::open(&bytes).unwrap();
        assert!(image.media().section(MediaSectionKind::PLANES).is_none());
        assert_eq!(image.plane(0).unwrap().bytes(), y);
        assert_eq!(image.plane(1).unwrap().bytes(), uv);
    }

    #[test]
    fn indexed_asset_round_trips_its_separate_color_table() {
        let surface =
            SurfaceDescriptor::new(3, 2, SampleLayout::I4, ColorDescription::SRGB).unwrap();
        let indices = [0x12, 0x30, 0x45, 0x60];
        let planes: &[&[u8]] = &[&indices];
        let colors = [0xa5; 64];
        let bytes = RawImageAsset::new(surface, planes)
            .with_color_table(&colors)
            .encode()
            .unwrap();
        let image = RawImageView::open(&bytes).unwrap();
        assert_eq!(image.color_table().unwrap().as_bytes(), colors);
        assert_eq!(image.plane(0).unwrap().bytes(), indices);
    }

    #[test]
    fn explicit_padding_is_zeroed_and_plane_records_round_trip() {
        let surface =
            SurfaceDescriptor::new(3, 2, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
        let plane = surface.plane(0).unwrap();
        let memory = PlaneMemoryLayout::builder(plane)
            .with_allocation_extent(4, 2)
            .with_stride(16)
            .with_data_offset(64)
            .with_alignment(64)
            .build()
            .unwrap();
        let pixels = [0x5a; 32];
        let planes: &[&[u8]] = &[&pixels];
        let memory_layouts = [memory];
        let bytes = RawImageAsset::new(surface, planes)
            .with_memory_layouts(&memory_layouts)
            .encode()
            .unwrap();
        let image = RawImageView::open(&bytes).unwrap();
        assert!(image.media().section(MediaSectionKind::PLANES).is_some());
        assert_eq!(image.plane(0).unwrap().memory(), memory);
        assert_eq!(image.plane(0).unwrap().bytes(), pixels);
        let data = image
            .media()
            .section(MediaSectionKind::DATA)
            .unwrap()
            .bytes();
        assert_eq!(&data[..64], &[0; 64]);
    }

    #[test]
    fn validation_precedes_capacity_and_preserves_output() {
        let surface =
            SurfaceDescriptor::new(2, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let short_plane = [0; 3];
        let short_planes: &[&[u8]] = &[&short_plane];
        let invalid = RawImageAsset::new(surface, short_planes);
        let mut out = [0xa5; 16];
        assert_eq!(
            invalid.encode_into(&mut out),
            Err(RawImageEncodeError::PlaneLengthMismatch {
                index: 0,
                expected: 4,
                actual: 3,
            })
        );
        assert_eq!(out, [0xa5; 16]);

        let pixels = [0; 4];
        let planes: &[&[u8]] = &[&pixels];
        let valid = RawImageAsset::new(surface, planes);
        let needed = valid.encoded_len().unwrap();
        assert_eq!(
            valid.encode_into(&mut out),
            Err(RawImageEncodeError::BufferTooSmall {
                needed,
                available: out.len(),
            })
        );
        assert_eq!(out, [0xa5; 16]);
    }

    #[test]
    fn plane_and_color_table_cardinality_are_exact() {
        let indexed =
            SurfaceDescriptor::new(1, 1, SampleLayout::I1, ColorDescription::SRGB).unwrap();
        let pixel = [0; 1];
        let planes: &[&[u8]] = &[&pixel];
        assert_eq!(
            RawImageAsset::new(indexed, planes).encoded_len(),
            Err(RawImageEncodeError::MissingColorTable)
        );
        assert_eq!(
            RawImageAsset::new(indexed, planes)
                .with_color_table(&[0; 7])
                .encoded_len(),
            Err(RawImageEncodeError::ColorTableLengthMismatch {
                expected: 8,
                actual: 7,
            })
        );

        let rgb =
            SurfaceDescriptor::new(1, 1, SampleLayout::RGB565, ColorDescription::SRGB).unwrap();
        let none: &[&[u8]] = &[];
        assert_eq!(
            RawImageAsset::new(rgb, none).encoded_len(),
            Err(RawImageEncodeError::PlaneCountMismatch {
                expected: 1,
                actual: 0,
            })
        );
    }

    #[test]
    fn invalid_explicit_layout_is_rechecked_against_the_surface() {
        let small = SurfaceDescriptor::new(2, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let large = SurfaceDescriptor::new(4, 4, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let memory = PlaneMemoryLayout::tight(small.plane(0).unwrap()).unwrap();
        let pixels = [0; 4];
        let planes: &[&[u8]] = &[&pixels];
        assert_eq!(
            RawImageAsset::new(large, planes)
                .with_memory_layouts(&[memory])
                .encoded_len(),
            Err(RawImageEncodeError::InvalidPlaneLayout {
                index: 0,
                error: PlaneMemoryError::AllocationWidthTooSmall {
                    minimum: 4,
                    actual: 2,
                },
            })
        );
    }

    #[test]
    fn exact_buffer_encoding_preserves_suffix() {
        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let pixel = [0x7f];
        let planes: &[&[u8]] = &[&pixel];
        let asset = RawImageAsset::new(surface, planes);
        let needed = asset.encoded_len().unwrap();
        let mut out = vec![0xa5; needed + 9];
        assert_eq!(asset.encode_into(&mut out), Ok(needed));
        assert_eq!(&out[needed..], &[0xa5; 9]);
        assert_eq!(
            RawImageView::open(&out[..needed])
                .unwrap()
                .plane(0)
                .unwrap()
                .bytes(),
            pixel
        );
    }
}
