use core::iter::FusedIterator;

use super::{PlaneMemoryError, PlaneMemoryLayout, SurfaceDescriptor};

/// Backend allocation constraints for every plane of a decoded surface.
///
/// Multiples apply independently to each derived plane. Byte alignments must
/// be powers of two; dimension and stride multiples may be any nonzero value.
/// Width is measured in plane elements, height in rows, and stride in bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceRequirements {
    base_alignment: u32,
    plane_alignment: u32,
    width_multiple: u32,
    height_multiple: u32,
    stride_multiple: u32,
}

impl SurfaceRequirements {
    pub const fn new() -> Self {
        Self {
            base_alignment: 1,
            plane_alignment: 1,
            width_multiple: 1,
            height_multiple: 1,
            stride_multiple: 1,
        }
    }

    pub const fn with_base_alignment(mut self, alignment: u32) -> Self {
        self.base_alignment = alignment;
        self
    }

    pub const fn with_plane_alignment(mut self, alignment: u32) -> Self {
        self.plane_alignment = alignment;
        self
    }

    pub const fn with_width_multiple(mut self, multiple: u32) -> Self {
        self.width_multiple = multiple;
        self
    }

    pub const fn with_height_multiple(mut self, multiple: u32) -> Self {
        self.height_multiple = multiple;
        self
    }

    pub const fn with_stride_multiple(mut self, multiple: u32) -> Self {
        self.stride_multiple = multiple;
        self
    }

    pub const fn base_alignment(self) -> u32 {
        self.base_alignment
    }

    pub const fn plane_alignment(self) -> u32 {
        self.plane_alignment
    }

    pub const fn width_multiple(self) -> u32 {
        self.width_multiple
    }

    pub const fn height_multiple(self) -> u32 {
        self.height_multiple
    }

    pub const fn stride_multiple(self) -> u32 {
        self.stride_multiple
    }

    fn validate(self) -> Result<(), SurfacePlanError> {
        if !valid_alignment(self.base_alignment) {
            return Err(SurfacePlanError::InvalidBaseAlignment(self.base_alignment));
        }
        if !valid_alignment(self.plane_alignment) {
            return Err(SurfacePlanError::InvalidPlaneAlignment(
                self.plane_alignment,
            ));
        }
        if self.width_multiple == 0 {
            return Err(SurfacePlanError::InvalidWidthMultiple);
        }
        if self.height_multiple == 0 {
            return Err(SurfacePlanError::InvalidHeightMultiple);
        }
        if self.stride_multiple == 0 {
            return Err(SurfacePlanError::InvalidStrideMultiple);
        }
        Ok(())
    }
}

impl Default for SurfaceRequirements {
    fn default() -> Self {
        Self::new()
    }
}

/// Total caller-buffer requirements for one planned decoded surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BufferRequirements {
    byte_len: usize,
    base_alignment: usize,
}

impl BufferRequirements {
    pub const fn byte_len(self) -> usize {
        self.byte_len
    }

    pub const fn base_alignment(self) -> usize {
        self.base_alignment
    }

    /// Validates size and the actual runtime address of a caller buffer.
    pub fn validate(self, buffer: &[u8]) -> Result<(), BufferRequirementError> {
        if buffer.len() < self.byte_len {
            return Err(BufferRequirementError::TooSmall {
                needed: self.byte_len,
                available: buffer.len(),
            });
        }
        if self.byte_len == 0 {
            return Ok(());
        }
        if buffer.as_ptr() as usize % self.base_alignment != 0 {
            return Err(BufferRequirementError::AddressUnaligned {
                address: buffer.as_ptr() as usize,
                alignment: self.base_alignment,
            });
        }
        Ok(())
    }
}

/// Deterministic physical layout for a decoded surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceMemoryPlan {
    surface: SurfaceDescriptor,
    requirements: SurfaceRequirements,
    byte_len: u32,
    base_alignment: u32,
}

impl SurfaceDescriptor {
    /// Plans a checked caller-owned allocation for this decoded surface.
    pub fn memory_plan(
        self,
        requirements: SurfaceRequirements,
    ) -> Result<SurfaceMemoryPlan, SurfacePlanError> {
        requirements.validate()?;
        let base_alignment = requirements
            .base_alignment
            .max(requirements.plane_alignment);
        let mut byte_len = 0;
        for index in 0..self.plane_count() {
            let plane = planned_plane(self, requirements, index, byte_len)?;
            byte_len = plane.data_end();
        }
        usize::try_from(byte_len).map_err(|_| SurfacePlanError::SizeOverflow)?;
        usize::try_from(base_alignment).map_err(|_| SurfacePlanError::SizeOverflow)?;
        Ok(SurfaceMemoryPlan {
            surface: self,
            requirements,
            byte_len,
            base_alignment,
        })
    }
}

impl SurfaceMemoryPlan {
    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }

    pub const fn requirements(self) -> SurfaceRequirements {
        self.requirements
    }

    pub const fn byte_len(self) -> u32 {
        self.byte_len
    }

    pub const fn base_alignment(self) -> u32 {
        self.base_alignment
    }

    pub const fn buffer_requirements(self) -> BufferRequirements {
        BufferRequirements {
            byte_len: self.byte_len as usize,
            base_alignment: self.base_alignment as usize,
        }
    }

    pub fn plane(self, index: u8) -> Option<PlaneMemoryLayout> {
        if index >= self.surface.plane_count() {
            return None;
        }
        let mut offset = 0;
        for current in 0..=index {
            let plane = planned_plane(self.surface, self.requirements, current, offset)
                .expect("validated surface memory plan");
            if current == index {
                return Some(plane);
            }
            offset = plane.data_end();
        }
        None
    }

    pub fn planes(self) -> SurfaceMemoryPlanes {
        SurfaceMemoryPlanes {
            plan: self,
            front: 0,
            back: self.surface.plane_count(),
        }
    }
}

/// Exact-size iterator over the physical planes of a memory plan.
#[derive(Clone, Debug)]
pub struct SurfaceMemoryPlanes {
    plan: SurfaceMemoryPlan,
    front: u8,
    back: u8,
}

impl Iterator for SurfaceMemoryPlanes {
    type Item = PlaneMemoryLayout;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        self.plan.plane(index)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for SurfaceMemoryPlanes {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.plan.plane(self.back)
    }
}

impl ExactSizeIterator for SurfaceMemoryPlanes {
    fn len(&self) -> usize {
        usize::from(self.back - self.front)
    }
}

impl FusedIterator for SurfaceMemoryPlanes {}

/// Failure while planning physical storage for a decoded surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SurfacePlanError {
    InvalidBaseAlignment(u32),
    InvalidPlaneAlignment(u32),
    InvalidWidthMultiple,
    InvalidHeightMultiple,
    InvalidStrideMultiple,
    InvalidPlane { index: u8, error: PlaneMemoryError },
    SizeOverflow,
}

/// Failure while validating a caller-owned output buffer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum BufferRequirementError {
    TooSmall { needed: usize, available: usize },
    AddressUnaligned { address: usize, alignment: usize },
}

fn planned_plane(
    surface: SurfaceDescriptor,
    requirements: SurfaceRequirements,
    index: u8,
    previous_end: u32,
) -> Result<PlaneMemoryLayout, SurfacePlanError> {
    let geometry = surface.plane(index).ok_or(SurfacePlanError::SizeOverflow)?;
    let allocation_width = round_up(geometry.width(), requirements.width_multiple)
        .ok_or(SurfacePlanError::SizeOverflow)?;
    let allocation_height = round_up(geometry.height(), requirements.height_multiple)
        .ok_or(SurfacePlanError::SizeOverflow)?;
    let minimum_stride =
        crate::format::minimum_stride_for_bits(allocation_width, geometry.bits_per_element())
            .ok_or(SurfacePlanError::SizeOverflow)?;
    let stride = round_up(minimum_stride, requirements.stride_multiple)
        .ok_or(SurfacePlanError::SizeOverflow)?;
    let data_offset = round_up(previous_end, requirements.plane_alignment)
        .ok_or(SurfacePlanError::SizeOverflow)?;
    PlaneMemoryLayout::builder(geometry)
        .with_allocation_extent(allocation_width, allocation_height)
        .with_stride(stride)
        .with_data_offset(data_offset)
        .with_alignment(requirements.plane_alignment)
        .build()
        .map_err(|error| SurfacePlanError::InvalidPlane { index, error })
}

const fn valid_alignment(alignment: u32) -> bool {
    alignment.is_power_of_two() && alignment <= (1 << 31)
}

fn round_up(value: u32, multiple: u32) -> Option<u32> {
    if multiple == 0 {
        return None;
    }
    value.div_ceil(multiple).checked_mul(multiple)
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::image::{ColorDescription, SampleLayout};

    #[test]
    fn default_plan_is_tight_and_contiguous() {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let plan = surface.memory_plan(SurfaceRequirements::new()).unwrap();
        let planes: alloc::vec::Vec<_> = plan.planes().collect();

        assert_eq!(planes.len(), 2);
        assert_eq!((planes[0].stride(), planes[0].data_offset()), (5, 0));
        assert_eq!((planes[1].stride(), planes[1].data_offset()), (6, 15));
        assert_eq!(plan.byte_len(), 27);
        assert_eq!(plan.base_alignment(), 1);
    }

    #[test]
    fn gpu_plan_rounds_dimensions_stride_and_plane_offsets_independently() {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let requirements = SurfaceRequirements::new()
            .with_base_alignment(64)
            .with_plane_alignment(64)
            .with_width_multiple(8)
            .with_height_multiple(2)
            .with_stride_multiple(64);
        let plan = surface.memory_plan(requirements).unwrap();
        let y = plan.plane(0).unwrap();
        let uv = plan.plane(1).unwrap();

        assert_eq!((y.allocation_width(), y.allocation_height()), (8, 4));
        assert_eq!((y.stride(), y.data_offset(), y.byte_len()), (64, 0, 256));
        assert_eq!((uv.allocation_width(), uv.allocation_height()), (8, 2));
        assert_eq!(
            (uv.stride(), uv.data_offset(), uv.byte_len()),
            (64, 256, 128)
        );
        assert_eq!(plan.byte_len(), 384);
        assert_eq!(plan.base_alignment(), 64);
    }

    #[test]
    fn bes_style_rgba_plan_keeps_logical_width_and_rounds_storage_width() {
        let surface =
            SurfaceDescriptor::new(319, 181, SampleLayout::RGBA8888, ColorDescription::SRGB)
                .unwrap();
        let plan = surface
            .memory_plan(
                SurfaceRequirements::new()
                    .with_base_alignment(64)
                    .with_plane_alignment(64)
                    .with_width_multiple(64)
                    .with_stride_multiple(64),
            )
            .unwrap();
        let plane = plan.plane(0).unwrap();

        assert_eq!((surface.width(), surface.height()), (319, 181));
        assert_eq!(plane.allocation_width(), 320);
        assert_eq!(plane.allocation_height(), 181);
        assert_eq!(plane.stride(), 1_280);
        assert_eq!(plane.data_offset(), 0);
        assert_eq!(plan.byte_len(), 231_680);
        assert_eq!(plan.buffer_requirements().base_alignment(), 64);
    }

    #[test]
    fn invalid_requirements_and_overflow_fail_before_a_plan_exists() {
        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        assert_eq!(
            surface.memory_plan(SurfaceRequirements::new().with_base_alignment(3)),
            Err(SurfacePlanError::InvalidBaseAlignment(3))
        );
        assert_eq!(
            surface.memory_plan(SurfaceRequirements::new().with_width_multiple(0)),
            Err(SurfacePlanError::InvalidWidthMultiple)
        );

        let huge =
            SurfaceDescriptor::new(u32::MAX, 1, SampleLayout::RGBA8888, ColorDescription::SRGB)
                .unwrap();
        assert_eq!(
            huge.memory_plan(SurfaceRequirements::new()),
            Err(SurfacePlanError::SizeOverflow)
        );

        let exact =
            SurfaceDescriptor::new(u32::MAX, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let plan = exact
            .memory_plan(
                SurfaceRequirements::new()
                    .with_width_multiple(3)
                    .with_stride_multiple(3),
            )
            .unwrap();
        assert_eq!(plan.byte_len(), u32::MAX);
        assert_eq!(
            exact.memory_plan(SurfaceRequirements::new().with_width_multiple(2)),
            Err(SurfacePlanError::SizeOverflow)
        );
    }

    #[test]
    fn buffer_validation_checks_length_and_real_address() {
        let surface =
            SurfaceDescriptor::new(8, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let plan = surface
            .memory_plan(SurfaceRequirements::new().with_base_alignment(64))
            .unwrap();
        let requirements = plan.buffer_requirements();
        let storage = vec![0; requirements.byte_len() + 64];
        let base = storage.as_ptr() as usize;
        let aligned_start = (64 - base % 64) % 64;
        let aligned = &storage[aligned_start..aligned_start + requirements.byte_len()];

        assert_eq!(requirements.validate(aligned), Ok(()));
        assert_eq!(
            requirements.validate(&aligned[..aligned.len() - 1]),
            Err(BufferRequirementError::TooSmall {
                needed: requirements.byte_len(),
                available: requirements.byte_len() - 1,
            })
        );
        let unaligned = &storage[aligned_start + 1..];
        assert_eq!(
            requirements.validate(unaligned),
            Err(BufferRequirementError::AddressUnaligned {
                address: unaligned.as_ptr() as usize,
                alignment: 64,
            })
        );

        let empty = SurfaceDescriptor::new(0, 0, SampleLayout::A8, ColorDescription::NONE)
            .unwrap()
            .memory_plan(SurfaceRequirements::new().with_base_alignment(64))
            .unwrap()
            .buffer_requirements();
        assert_eq!(empty.validate(&[]), Ok(()));
    }
}
