use core::ops::Range;

use super::PlaneGeometry;
use crate::ByteAlignment;
use crate::format::minimum_stride_for_bits;
use crate::wire::{read_u16_le, read_u32_le, write_u16_le, write_u32_le};

pub(crate) const PLANE_RECORD_LEN: usize = 24;

/// Open flags attached to one physical plane-memory record.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PlaneMemoryFlags(u16);

impl PlaneMemoryFlags {
    pub const NONE: Self = Self(0);

    pub const fn from_bits_retain(bits: u16) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u16 {
        self.0
    }
}

/// Physical allocation and byte layout for one decoded plane.
///
/// The record does not repeat the plane role, logical dimensions, sample
/// depth, or subsampling. Those values come from [`PlaneGeometry`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaneMemoryLayout {
    allocation_width: u32,
    allocation_height: u32,
    stride: u32,
    data_offset: u32,
    alignment_log2: u8,
    flags: PlaneMemoryFlags,
}

impl PlaneMemoryLayout {
    /// Starts a checked physical-layout builder with tight allocation defaults.
    pub const fn builder(plane: PlaneGeometry) -> PlaneMemoryBuilder {
        PlaneMemoryBuilder {
            plane,
            allocation_width: plane.width(),
            allocation_height: plane.height(),
            stride: None,
            data_offset: 0,
            alignment: ByteAlignment::ONE,
            flags: PlaneMemoryFlags::NONE,
        }
    }

    /// Builds the tightly packed canonical layout at DATA offset zero.
    pub fn tight(plane: PlaneGeometry) -> Result<Self, PlaneMemoryError> {
        Self::builder(plane).build()
    }

    pub const fn allocation_width(self) -> u32 {
        self.allocation_width
    }

    pub const fn allocation_height(self) -> u32 {
        self.allocation_height
    }

    pub const fn stride(self) -> u32 {
        self.stride
    }

    /// Byte offset relative to the containing DATA section or caller-owned buffer.
    pub const fn data_offset(self) -> u32 {
        self.data_offset
    }

    pub const fn required_alignment(self) -> ByteAlignment {
        match ByteAlignment::new(1u32 << self.alignment_log2) {
            Ok(alignment) => alignment,
            Err(_) => unreachable!(),
        }
    }

    pub const fn flags(self) -> PlaneMemoryFlags {
        self.flags
    }

    /// Revalidates this physical layout against a derived logical plane.
    pub fn validate_for(self, plane: PlaneGeometry) -> Result<(), PlaneMemoryError> {
        Self::builder(plane)
            .with_allocation_extent(self.allocation_width, self.allocation_height)
            .with_stride(self.stride)
            .with_data_offset(self.data_offset)
            .with_alignment(self.required_alignment())
            .with_flags(self.flags)
            .build()
            .map(|_| ())
    }

    /// Physical byte length including row padding and padded allocation rows.
    pub const fn byte_len(self) -> u32 {
        self.stride * self.allocation_height
    }

    pub const fn data_end(self) -> u32 {
        self.data_offset + self.byte_len()
    }

    pub fn data_range(self) -> Range<u32> {
        self.data_offset..self.data_end()
    }

    /// Returns the plane bytes even when their runtime address is unsuitable
    /// for a direct DMA or device path.
    pub fn bytes(self, data_section: &[u8]) -> Option<&[u8]> {
        let start = usize::try_from(self.data_offset).ok()?;
        let len = usize::try_from(self.byte_len()).ok()?;
        data_section.get(start..start.checked_add(len)?)
    }

    /// Checks the promised file or flash address of this plane.
    pub const fn file_address_is_aligned(self, data_section_offset: u32) -> bool {
        let Some(address) = data_section_offset.checked_add(self.data_offset) else {
            return false;
        };
        address % self.required_alignment().get() == 0
    }

    /// Checks the actual in-memory address without treating allocator behavior
    /// as a format guarantee.
    pub fn runtime_address_is_aligned(self, data_section: &[u8]) -> bool {
        let Some(bytes) = self.bytes(data_section) else {
            return false;
        };
        bytes.as_ptr() as usize
            % usize::try_from(self.required_alignment().get()).expect("u32 fits usize")
            == 0
    }

    /// Decodes and validates one fixed-width PLANES section record against its
    /// derived logical plane.
    pub(crate) fn from_record(
        plane: PlaneGeometry,
        bytes: &[u8],
    ) -> Result<Self, PlaneMemoryRecordError> {
        if bytes.len() < PLANE_RECORD_LEN {
            return Err(PlaneMemoryRecordError::Truncated {
                needed: PLANE_RECORD_LEN,
                available: bytes.len(),
            });
        }
        for (relative, byte) in bytes[19..PLANE_RECORD_LEN].iter().enumerate() {
            if *byte != 0 {
                let offset = 19 + relative;
                return Err(PlaneMemoryRecordError::ReservedNonZero { offset });
            }
        }

        let allocation_width = read_u32_le(bytes, 0).expect("complete plane record");
        let allocation_height = read_u32_le(bytes, 4).expect("complete plane record");
        let stride = read_u32_le(bytes, 8).expect("complete plane record");
        let data_offset = read_u32_le(bytes, 12).expect("complete plane record");
        let flags = PlaneMemoryFlags::from_bits_retain(
            read_u16_le(bytes, 16).expect("complete plane record"),
        );
        let alignment_log2 = bytes[18];
        if alignment_log2 > 31 {
            return Err(PlaneMemoryRecordError::InvalidLayout(
                PlaneMemoryError::InvalidAlignmentLog2(alignment_log2),
            ));
        }

        Self::builder(plane)
            .with_allocation_extent(allocation_width, allocation_height)
            .with_stride(stride)
            .with_data_offset(data_offset)
            .with_alignment(
                ByteAlignment::new(1u32 << alignment_log2)
                    .expect("validated plane alignment exponent"),
            )
            .with_flags(flags)
            .build()
            .map_err(PlaneMemoryRecordError::InvalidLayout)
    }

    pub(crate) fn encode_record(self) -> [u8; PLANE_RECORD_LEN] {
        let mut record = [0; PLANE_RECORD_LEN];
        write_u32_le(&mut record, 0, self.allocation_width);
        write_u32_le(&mut record, 4, self.allocation_height);
        write_u32_le(&mut record, 8, self.stride);
        write_u32_le(&mut record, 12, self.data_offset);
        write_u16_le(&mut record, 16, self.flags.bits());
        record[18] = self.alignment_log2;
        record
    }
}

/// Builder for a checked physical plane layout.
#[derive(Clone, Copy, Debug)]
pub struct PlaneMemoryBuilder {
    plane: PlaneGeometry,
    allocation_width: u32,
    allocation_height: u32,
    stride: Option<u32>,
    data_offset: u32,
    alignment: ByteAlignment,
    flags: PlaneMemoryFlags,
}

impl PlaneMemoryBuilder {
    pub const fn with_allocation_width(mut self, width: u32) -> Self {
        self.allocation_width = width;
        self
    }

    pub const fn with_allocation_height(mut self, height: u32) -> Self {
        self.allocation_height = height;
        self
    }

    pub const fn with_allocation_extent(mut self, width: u32, height: u32) -> Self {
        self.allocation_width = width;
        self.allocation_height = height;
        self
    }

    /// Sets an explicit byte stride. If omitted, `build` derives the minimum
    /// stride from the final allocation width and plane sample depth.
    pub const fn with_stride(mut self, stride: u32) -> Self {
        self.stride = Some(stride);
        self
    }

    pub const fn with_data_offset(mut self, offset: u32) -> Self {
        self.data_offset = offset;
        self
    }

    /// Sets a power-of-two byte alignment for the plane start address.
    pub const fn with_alignment(mut self, alignment: ByteAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    pub const fn with_flags(mut self, flags: PlaneMemoryFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn build(self) -> Result<PlaneMemoryLayout, PlaneMemoryError> {
        if self.allocation_width < self.plane.width() {
            return Err(PlaneMemoryError::AllocationWidthTooSmall {
                minimum: self.plane.width(),
                actual: self.allocation_width,
            });
        }
        if self.allocation_height < self.plane.height() {
            return Err(PlaneMemoryError::AllocationHeightTooSmall {
                minimum: self.plane.height(),
                actual: self.allocation_height,
            });
        }

        let minimum_stride =
            minimum_stride_for_bits(self.allocation_width, self.plane.bits_per_element())
                .ok_or(PlaneMemoryError::SizeOverflow)?;
        let stride = self.stride.unwrap_or(minimum_stride);
        if stride < minimum_stride {
            return Err(PlaneMemoryError::StrideTooSmall {
                minimum: minimum_stride,
                actual: stride,
            });
        }
        let byte_len = stride
            .checked_mul(self.allocation_height)
            .ok_or(PlaneMemoryError::SizeOverflow)?;
        self.data_offset
            .checked_add(byte_len)
            .ok_or(PlaneMemoryError::SizeOverflow)?;

        if self.data_offset % self.alignment.get() != 0 {
            return Err(PlaneMemoryError::DataOffsetUnaligned {
                offset: self.data_offset,
                alignment: self.alignment,
            });
        }

        Ok(PlaneMemoryLayout {
            allocation_width: self.allocation_width,
            allocation_height: self.allocation_height,
            stride,
            data_offset: self.data_offset,
            alignment_log2: self.alignment.log2(),
            flags: self.flags,
        })
    }
}

/// Invalid physical memory layout for one derived plane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PlaneMemoryError {
    AllocationWidthTooSmall {
        minimum: u32,
        actual: u32,
    },
    AllocationHeightTooSmall {
        minimum: u32,
        actual: u32,
    },
    StrideTooSmall {
        minimum: u32,
        actual: u32,
    },
    InvalidAlignmentLog2(u8),
    DataOffsetUnaligned {
        offset: u32,
        alignment: ByteAlignment,
    },
    SizeOverflow,
}

/// Failure while decoding a PLANES section record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PlaneMemoryRecordError {
    Truncated { needed: usize, available: usize },
    ReservedNonZero { offset: usize },
    InvalidLayout(PlaneMemoryError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::ColorDescription;
    use crate::image::{SampleLayout, SurfaceDescriptor};

    fn rgba_plane(width: u32, height: u32) -> PlaneGeometry {
        SampleLayout::RGBA8888
            .plane_geometry(width, height, 0)
            .unwrap()
    }

    #[test]
    fn tight_layout_uses_the_shared_minimum_stride() {
        let plane = rgba_plane(13, 7);
        let memory = PlaneMemoryLayout::tight(plane).unwrap();
        assert_eq!(memory.allocation_width(), 13);
        assert_eq!(memory.allocation_height(), 7);
        assert_eq!(memory.stride(), 52);
        assert_eq!(memory.byte_len(), 364);
        assert_eq!(memory.data_range(), 0..364);
        assert_eq!(memory.required_alignment(), 1);
    }

    #[test]
    fn padded_gpu_layout_keeps_logical_and_allocation_geometry_separate() {
        let surface =
            SurfaceDescriptor::new(319, 181, SampleLayout::RGBA8888, ColorDescription::SRGB)
                .unwrap();
        let memory = PlaneMemoryLayout::builder(surface.plane(0).unwrap())
            .with_allocation_extent(320, 192)
            .with_stride(1_280)
            .with_data_offset(64)
            .with_alignment(crate::ByteAlignment::new(64).unwrap())
            .build()
            .unwrap();

        assert_eq!((surface.width(), surface.height()), (319, 181));
        assert_eq!(
            (memory.allocation_width(), memory.allocation_height()),
            (320, 192)
        );
        assert_eq!(memory.stride(), 1_280);
        assert_eq!(memory.data_range(), 64..245_824);
        assert!(memory.file_address_is_aligned(128));
        assert!(!memory.file_address_is_aligned(132));
    }

    #[test]
    fn builder_rejects_every_invalid_physical_constraint() {
        let plane = rgba_plane(319, 181);
        assert_eq!(
            PlaneMemoryLayout::builder(plane)
                .with_allocation_width(318)
                .build(),
            Err(PlaneMemoryError::AllocationWidthTooSmall {
                minimum: 319,
                actual: 318,
            })
        );
        assert_eq!(
            PlaneMemoryLayout::builder(plane)
                .with_allocation_height(180)
                .build(),
            Err(PlaneMemoryError::AllocationHeightTooSmall {
                minimum: 181,
                actual: 180,
            })
        );
        assert_eq!(
            PlaneMemoryLayout::builder(plane).with_stride(1_275).build(),
            Err(PlaneMemoryError::StrideTooSmall {
                minimum: 1_276,
                actual: 1_275,
            })
        );
        assert!(crate::ByteAlignment::new(48).is_err());
        assert_eq!(
            PlaneMemoryLayout::builder(plane)
                .with_alignment(crate::ByteAlignment::new(64).unwrap())
                .with_data_offset(32)
                .build(),
            Err(PlaneMemoryError::DataOffsetUnaligned {
                offset: 32,
                alignment: crate::ByteAlignment::new(64).unwrap(),
            })
        );
    }

    #[test]
    fn overflow_is_rejected_before_a_layout_exists() {
        let plane = SampleLayout::P016
            .plane_geometry(u32::MAX, u32::MAX, 1)
            .unwrap();
        assert_eq!(
            PlaneMemoryLayout::builder(plane).build(),
            Err(PlaneMemoryError::SizeOverflow)
        );
    }

    #[test]
    fn record_round_trip_preserves_only_physical_fields() {
        let plane = SampleLayout::P010.plane_geometry(319, 181, 1).unwrap();
        let memory = PlaneMemoryLayout::builder(plane)
            .with_allocation_extent(160, 96)
            .with_stride(640)
            .with_data_offset(1_280)
            .with_alignment(crate::ByteAlignment::new(256).unwrap())
            .with_flags(PlaneMemoryFlags::from_bits_retain(0xa501))
            .build()
            .unwrap();
        let bytes = memory.encode_record();
        assert_eq!(PlaneMemoryLayout::from_record(plane, &bytes), Ok(memory));
        assert_eq!(&bytes[19..], &[0; 5]);
    }

    #[test]
    fn record_boundaries_and_reserved_bytes_are_checked() {
        let plane = rgba_plane(2, 2);
        let memory = PlaneMemoryLayout::tight(plane).unwrap();
        let bytes = memory.encode_record();

        for available in 0..PLANE_RECORD_LEN {
            assert_eq!(
                PlaneMemoryLayout::from_record(plane, &bytes[..available]),
                Err(PlaneMemoryRecordError::Truncated {
                    needed: PLANE_RECORD_LEN,
                    available,
                })
            );
        }
        for offset in 19..PLANE_RECORD_LEN {
            let mut reserved = bytes;
            reserved[offset] = 1;
            assert_eq!(
                PlaneMemoryLayout::from_record(plane, &reserved),
                Err(PlaneMemoryRecordError::ReservedNonZero { offset })
            );
        }
    }

    #[test]
    fn file_and_runtime_alignment_are_independent() {
        #[repr(align(64))]
        struct Aligned([u8; 128]);

        let plane = SampleLayout::A8.plane_geometry(16, 4, 0).unwrap();
        let memory = PlaneMemoryLayout::builder(plane)
            .with_stride(16)
            .with_alignment(crate::ByteAlignment::new(64).unwrap())
            .build()
            .unwrap();
        let aligned = Aligned([0; 128]);
        assert!(memory.runtime_address_is_aligned(&aligned.0));
        assert!(!memory.runtime_address_is_aligned(&aligned.0[1..]));
        assert!(memory.file_address_is_aligned(0));
        assert!(!memory.file_address_is_aligned(1));
        assert_eq!(memory.bytes(&aligned.0).map(<[u8]>::len), Some(64));
    }
}
