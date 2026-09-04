use super::{DecodeUnitRef, ReferenceMode, UnitMemoryPlan, output::UnitOutput};
use crate::{
    coding::{Pixel, PixelDecodePlan, PixelError, Rle, RleDecodePlan, RleError},
    image::{BufferRequirementError, SurfacePlanError, SurfacePlane, SurfaceRequirements},
    media::CodingId,
};

#[cfg(test)]
mod tests;

/// Validated scalar execution into independent, caller-owned unit storage.
///
/// PIXEL supports RGB888/RGBA8888; RLE covers selected tight plane rows. Other
/// coding profiles and reference modes are rejected during planning. Media
/// integrity is a separate gate, such as `ImageGroups::validate_unit`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitDecodePlan<'a> {
    memory: UnitMemoryPlan,
    decoder: Decoder<'a>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Decoder<'a> {
    Pixel(PixelDecodePlan<'a>),
    Rle(RleDecodePlan<'a>),
}

impl<'a> DecodeUnitRef<'a> {
    /// Validates profile syntax and plans a complete scalar-decoded unit.
    ///
    /// Logical output size comes from checked local plane geometry. Compressed bytes
    /// are read bytewise without hardware input-alignment requirements; the
    /// stored alignment promise remains separately inspectable on this unit.
    pub fn decode_plan(
        self,
        requirements: SurfaceRequirements,
    ) -> Result<UnitDecodePlan<'a>, UnitDecodeError> {
        if self.reference != ReferenceMode::Independent {
            return Err(UnitDecodeError::UnsupportedReference(self.reference));
        }
        if !matches!(self.coding.id(), CodingId::PIXEL | CodingId::RLE) {
            return Err(UnitDecodeError::UnsupportedCoding(self.coding.id()));
        }
        let memory = self
            .memory_plan(requirements)
            .map_err(UnitDecodeError::Memory)?;
        let decoder = if self.coding.id() == CodingId::PIXEL {
            let codec = Pixel::from_record(self.coding, self.surface.sample_layout())
                .map_err(UnitDecodeError::Pixel)?;
            let geometry = memory.plane(0).expect("pixel plane").geometry();
            let count = usize::try_from(u64::from(geometry.width()) * u64::from(geometry.height()))
                .map_err(|_| UnitDecodeError::Memory(SurfacePlanError::SizeOverflow))?;
            Decoder::Pixel(
                codec
                    .plan(self.data, count)
                    .map_err(UnitDecodeError::Pixel)?,
            )
        } else {
            let codec = Rle::from_record(self.coding).map_err(UnitDecodeError::Rle)?;
            Decoder::Rle(
                codec
                    .plan(self.data, memory.sample_byte_len())
                    .map_err(UnitDecodeError::Rle)?,
            )
        };
        Ok(UnitDecodePlan { memory, decoder })
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
        let mut writer = UnitOutput::new(self.memory, output);
        match self.decoder {
            Decoder::Pixel(plan) => {
                let channels = usize::from(
                    self.memory
                        .plane(0)
                        .expect("pixel plane")
                        .geometry()
                        .bits_per_element(),
                ) / 8;
                plan.for_each_run(|value, count| writer.repeat(&value[..channels], count));
            }
            Decoder::Rle(plan) => plan.for_each_block(|bytes, repeat| writer.repeat(bytes, repeat)),
        }
        writer.finish();
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
    Rle(RleError),
    Memory(SurfacePlanError),
    Output(BufferRequirementError),
}
