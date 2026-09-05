use super::{DecodeUnitRef, UnitMemoryPlan, output::UnitOutput};
use crate::{
    coding::{
        Frequency, FrequencyError, FrequencyGeometry, Lz4, Lz4DecodePlan, Lz4Error, Pixel,
        PixelDecodePlan, PixelError, Rle, RleDecodePlan, RleError,
    },
    image::{
        BufferRequirementError, SampleLayout, SurfacePlanError, SurfacePlane, SurfaceRequirements,
    },
    media::{CodingId, CodingRecord},
};

#[cfg(test)]
mod frequency_tests;
#[cfg(test)]
mod lz4_tests;
#[cfg(test)]
mod raw_tests;
#[cfg(test)]
mod tests;

/// Validated scalar execution into independent, caller-owned unit storage.
///
/// PIXEL supports RGB888/RGBA8888; RAW/RLE/LZ4 and frequency profiles cover
/// selected tight plane rows. The group reference mode determines how decoded
/// samples compose into a frame; it does not change these replacement codecs.
/// Other coding profiles are rejected during planning. Media integrity is a
/// separate gate, such as `ImageGroups::validate_unit`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnitDecodePlan<'a> {
    memory: UnitMemoryPlan,
    decoder: Decoder<'a>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Decoder<'a> {
    Raw(&'a [u8]),
    Pixel(PixelDecodePlan<'a>),
    Rle(RleDecodePlan<'a>),
    Lz4(Lz4DecodePlan<'a>),
    Frequency(FrequencyUnitDecodePlan<'a>),
}

#[derive(Clone, Copy)]
pub(crate) enum ScalarProfile {
    Raw,
    Pixel(Pixel),
    Rle(Rle),
    Lz4(Lz4),
    Frequency(Frequency),
}

impl ScalarProfile {
    pub(crate) fn new(
        coding: CodingRecord<'_>,
        layout: SampleLayout,
    ) -> Result<Self, UnitDecodeError> {
        match coding.id() {
            CodingId::RAW => {
                if coding.revision() != CodingRecord::RAW.revision() {
                    return Err(UnitDecodeError::UnsupportedRevision {
                        coding: coding.id(),
                        revision: coding.revision(),
                    });
                }
                if !coding.params().is_empty() {
                    return Err(UnitDecodeError::UnexpectedParameters(coding.id()));
                }
                Ok(Self::Raw)
            }
            CodingId::PIXEL => Pixel::from_record(coding, layout)
                .map(Self::Pixel)
                .map_err(UnitDecodeError::Pixel),
            CodingId::RLE => Rle::from_record(coding)
                .map(Self::Rle)
                .map_err(UnitDecodeError::Rle),
            CodingId::LZ4 => Lz4::from_record(coding)
                .map(Self::Lz4)
                .map_err(UnitDecodeError::Lz4),
            CodingId::FREQUENCY_REVERSIBLE | CodingId::FREQUENCY_QUANTIZED => {
                Frequency::from_record(coding)
                    .map(Self::Frequency)
                    .map_err(UnitDecodeError::Frequency)
            }
            id => Err(UnitDecodeError::UnsupportedCoding(id)),
        }
    }

    pub(crate) fn plan(
        self,
        data: &[u8],
        memory: UnitMemoryPlan,
    ) -> Result<UnitDecodePlan<'_>, UnitDecodeError> {
        let decoder = match self {
            Self::Raw => {
                let expected = memory.sample_byte_len();
                if data.len() != expected {
                    return Err(UnitDecodeError::SampleLengthMismatch {
                        expected,
                        actual: data.len(),
                    });
                }
                Decoder::Raw(data)
            }
            Self::Pixel(codec) => {
                let geometry = memory.plane(0).expect("pixel plane").geometry();
                let count =
                    usize::try_from(u64::from(geometry.width()) * u64::from(geometry.height()))
                        .map_err(|_| UnitDecodeError::Memory(SurfacePlanError::SizeOverflow))?;
                Decoder::Pixel(codec.plan(data, count).map_err(UnitDecodeError::Pixel)?)
            }
            Self::Rle(codec) => Decoder::Rle(
                codec
                    .plan(data, memory.sample_byte_len())
                    .map_err(UnitDecodeError::Rle)?,
            ),
            Self::Lz4(codec) => Decoder::Lz4(
                codec
                    .plan(data, memory.sample_byte_len())
                    .map_err(UnitDecodeError::Lz4)?,
            ),
            Self::Frequency(codec) => Decoder::Frequency(
                FrequencyUnitDecodePlan::new(codec, data, memory)
                    .map_err(UnitDecodeError::Frequency)?,
            ),
        };
        Ok(UnitDecodePlan { memory, decoder })
    }

    pub(crate) fn extra_work(self, memory: UnitMemoryPlan) -> Result<u64, UnitDecodeError> {
        let Self::Frequency(_) = self else {
            return Ok(0);
        };
        let mut coefficients = 0u64;
        for plane in memory.planes() {
            let geometry = FrequencyGeometry::for_plane(
                memory.source_surface().sample_layout(),
                plane.index(),
                plane.geometry().width(),
                plane.geometry().height(),
            )
            .map_err(UnitDecodeError::Frequency)?;
            coefficients = coefficients
                .checked_add(
                    geometry
                        .coefficient_count()
                        .map_err(UnitDecodeError::Frequency)? as u64,
                )
                .ok_or(UnitDecodeError::Frequency(FrequencyError::SizeOverflow))?;
        }
        coefficients
            .checked_mul(16)
            .ok_or(UnitDecodeError::Frequency(FrequencyError::SizeOverflow))
    }
}

impl<'a> DecodeUnitRef<'a> {
    /// Validates profile syntax and plans a complete scalar-decoded unit.
    ///
    /// Logical output size comes from checked local plane geometry. Stored bytes
    /// are read bytewise without hardware input-alignment requirements; the
    /// stored alignment promise remains separately inspectable on this unit.
    pub fn decode_plan(
        self,
        requirements: SurfaceRequirements,
    ) -> Result<UnitDecodePlan<'a>, UnitDecodeError> {
        let profile = ScalarProfile::new(self.coding, self.surface.sample_layout())?;
        let memory = self
            .memory_plan(requirements)
            .map_err(UnitDecodeError::Memory)?;
        profile.plan(self.data, memory)
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
        if let Decoder::Frequency(plan) = self.decoder {
            plan.decode_tight_into(&mut output[..self.memory.sample_byte_len()])
                .expect("validated frequency unit");
            UnitOutput::expand_tight(self.memory, output);
            return Ok(DecodedUnit {
                memory: self.memory,
                bytes: output,
            });
        }
        let mut writer = UnitOutput::new(self.memory, output);
        match self.decoder {
            Decoder::Raw(bytes) => writer.write(bytes),
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
            Decoder::Lz4(plan) => plan.for_each_sequence(|bytes, matched| {
                writer.write(bytes);
                if let Some((distance, len)) = matched {
                    writer.copy_match(distance, len);
                }
            }),
            Decoder::Frequency(_) => unreachable!("handled before sequential output"),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FrequencyUnitDecodePlan<'a> {
    codec: Frequency,
    input: &'a [u8],
    memory: UnitMemoryPlan,
}

impl<'a> FrequencyUnitDecodePlan<'a> {
    fn new(
        codec: Frequency,
        input: &'a [u8],
        memory: UnitMemoryPlan,
    ) -> Result<Self, FrequencyError> {
        let mut remaining = input;
        for plane in memory.planes() {
            let geometry = FrequencyGeometry::for_plane(
                memory.source_surface().sample_layout(),
                plane.index(),
                plane.geometry().width(),
                plane.geometry().height(),
            )?;
            let (_, consumed) = codec.plan_prefix(remaining, geometry)?;
            remaining = &remaining[consumed..];
        }
        if !remaining.is_empty() {
            return Err(FrequencyError::TrailingData {
                offset: input.len() - remaining.len(),
            });
        }
        Ok(Self {
            codec,
            input,
            memory,
        })
    }

    fn decode_tight_into(self, output: &mut [u8]) -> Result<(), FrequencyError> {
        debug_assert_eq!(output.len(), self.memory.sample_byte_len());
        let mut input = self.input;
        let mut output_offset = 0;
        for plane in self.memory.planes() {
            let geometry = FrequencyGeometry::for_plane(
                self.memory.source_surface().sample_layout(),
                plane.index(),
                plane.geometry().width(),
                plane.geometry().height(),
            )?;
            let (plan, consumed) = self.codec.plan_prefix(input, geometry)?;
            let len = plan.decoded_len();
            plan.decode_into(&mut output[output_offset..output_offset + len])?;
            input = &input[consumed..];
            output_offset += len;
        }
        debug_assert!(input.is_empty());
        debug_assert_eq!(output_offset, output.len());
        Ok(())
    }
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
    UnsupportedRevision {
        coding: CodingId,
        revision: u16,
    },
    UnexpectedParameters(CodingId),
    /// Tight selected-plane sample bytes differ from the stored RAW unit length.
    SampleLengthMismatch {
        expected: usize,
        actual: usize,
    },
    Pixel(PixelError),
    Rle(RleError),
    Lz4(Lz4Error),
    Frequency(FrequencyError),
    Memory(SurfacePlanError),
    Output(BufferRequirementError),
}
