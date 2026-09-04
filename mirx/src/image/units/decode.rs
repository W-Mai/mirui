use super::{DecodeUnitRef, ReferenceMode, UnitMemoryPlan};
use crate::{
    coding::{Pixel, PixelDecodePlan, PixelError},
    image::{BufferRequirementError, SurfacePlanError, SurfacePlane, SurfaceRequirements},
    media::CodingId,
};

#[cfg(test)]
mod tests;

/// Validated scalar execution into independent, caller-owned unit storage.
///
/// PIXEL streams support complete independent RGB888/RGBA8888 units. Other
/// coding profiles and reference modes are rejected during planning. Media
/// integrity is a separate gate, such as `ImageGroups::validate_unit`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitDecodePlan<'a> {
    memory: UnitMemoryPlan,
    pixels: PixelDecodePlan<'a>,
}

impl<'a> DecodeUnitRef<'a> {
    /// Validates profile syntax and plans a complete scalar-decoded unit.
    ///
    /// Pixel count comes from checked local plane geometry. Compressed bytes
    /// are read bytewise without hardware input-alignment requirements; the
    /// stored alignment promise remains separately inspectable on this unit.
    pub fn decode_plan(
        self,
        requirements: SurfaceRequirements,
    ) -> Result<UnitDecodePlan<'a>, UnitDecodeError> {
        if self.reference != ReferenceMode::Independent {
            return Err(UnitDecodeError::UnsupportedReference(self.reference));
        }
        if self.coding.id() != CodingId::PIXEL {
            return Err(UnitDecodeError::UnsupportedCoding(self.coding.id()));
        }
        let codec = Pixel::from_record(self.coding, self.surface.sample_layout())
            .map_err(UnitDecodeError::Pixel)?;
        let memory = self
            .memory_plan(requirements)
            .map_err(UnitDecodeError::Memory)?;
        // RGB/RGBA layouts have one selected plane, including planar groups.
        let geometry = memory.plane(0).expect("pixel plane").geometry();
        let count = usize::try_from(u64::from(geometry.width()) * u64::from(geometry.height()))
            .map_err(|_| UnitDecodeError::Memory(SurfacePlanError::SizeOverflow))?;
        let pixels = codec
            .plan(self.data, count)
            .map_err(UnitDecodeError::Pixel)?;
        Ok(UnitDecodePlan { memory, pixels })
    }
}

impl UnitDecodePlan<'_> {
    pub const fn memory_plan(self) -> UnitMemoryPlan {
        self.memory
    }

    /// Decodes directly into planned rows, without a tight staging buffer.
    ///
    /// Actual address and capacity are checked before any write. Allocation
    /// padding is zeroed; the suffix is unchanged. The result borrows only the
    /// output, so it can outlive the encoded input and this plan.
    pub fn decode_into(self, output: &mut [u8]) -> Result<DecodedUnit<'_>, UnitDecodeError> {
        self.memory
            .buffer_requirements()
            .validate(output)
            .map_err(UnitDecodeError::Output)?;
        let output = &mut output[..self.memory.byte_len() as usize];
        output.fill(0);
        let plane = self.memory.plane(0).expect("pixel plane");
        let width = plane.geometry().width() as usize;
        let channels = usize::from(plane.geometry().bits_per_element()) / 8;
        let stride = plane.memory().stride() as usize;
        let offset = plane.memory().data_offset() as usize;
        let mut row = 0;
        let mut column = 0;
        self.pixels.for_each_run(|value, mut remaining| {
            while remaining > 0 {
                let count = remaining.min(width - column);
                let start = offset + row * stride + column * channels;
                let end = start + count * channels;
                for pixel in output[start..end].chunks_exact_mut(channels) {
                    pixel.copy_from_slice(&value[..channels]);
                }
                remaining -= count;
                column += count;
                if column == width {
                    column = 0;
                    row += 1;
                }
            }
        });
        Ok(DecodedUnit {
            memory: self.memory,
            bytes: output,
        })
    }
}

/// Borrowed decoded unit planes, independent of encoded source lifetimes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedUnit<'a> {
    memory: UnitMemoryPlan,
    bytes: &'a [u8],
}
impl<'a> DecodedUnit<'a> {
    pub const fn memory_plan(self) -> UnitMemoryPlan {
        self.memory
    }

    /// Looks up an original surface plane index, retaining its local geometry.
    pub fn plane(self, index: u8) -> Option<SurfacePlane<'a>> {
        let plane = self.memory.plane(index)?;
        Some(SurfacePlane {
            geometry: plane.geometry(),
            memory: plane.memory(),
            bytes: plane
                .memory()
                .bytes(self.bytes)
                .expect("planned decoded plane"),
        })
    }

    pub fn planes(
        self,
    ) -> impl DoubleEndedIterator<Item = SurfacePlane<'a>> + ExactSizeIterator + core::iter::FusedIterator
    {
        self.memory
            .planes()
            .map(move |plane| self.plane(plane.index()).expect("selected decoded plane"))
    }
}

/// Unsupported unit execution or invalid syntax, memory requirements, or output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum UnitDecodeError {
    UnsupportedCoding(CodingId),
    UnsupportedReference(ReferenceMode),
    Pixel(PixelError),
    Memory(SurfacePlanError),
    Output(BufferRequirementError),
}
