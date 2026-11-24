use super::{ChunkRef, ContainerHeader, PayloadLimits, Reader};
use crate::ChunkType;
use crate::font::{FontError, FontView};
use crate::frames::FramesError;
use crate::image::{ImageReadError, ImageRef};
use crate::meta::MetaDecodeError;
use crate::palette::PaletteDecodeError;
use crate::reader::ReadError;
use crate::scene::{Scene, VectorReadError};

/// Source location of a payload validation result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PayloadLocation {
    Chunk {
        index: u16,
        chunk_type: ChunkType,
        payload_offset: u32,
    },
}

/// Semantic failure reported by bounded payload validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PayloadValidationFailure {
    UnsupportedStandardPayload,
    Image(ImageReadError),
    Font(FontError),
    Vector(VectorReadError),
    Meta(MetaDecodeError),
    Palette(PaletteDecodeError),
    Frames(FramesError),
}

/// A payload validation failure bound to its MIRX source location.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PayloadValidationError {
    location: PayloadLocation,
    failure: PayloadValidationFailure,
}

impl PayloadValidationError {
    pub const fn location(&self) -> PayloadLocation {
        self.location
    }

    pub const fn failure(&self) -> PayloadValidationFailure {
        self.failure
    }

    fn at_chunk(chunk: ChunkRef<'_>, failure: PayloadValidationFailure) -> Self {
        Self {
            location: PayloadLocation::Chunk {
                index: u16::try_from(chunk.index()).expect("validated MIRX chunk count"),
                chunk_type: chunk.chunk_type(),
                payload_offset: chunk.payload_offset(),
            },
            failure,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreflightStatus {
    Validated,
    UnsupportedStandard,
    Custom,
}

impl<'a> Reader<'a> {
    /// Validates every implemented standard payload without allocating.
    ///
    /// Custom chunk types are skipped. A standard type whose preflight is not
    /// implemented is reported explicitly instead of being treated as valid.
    pub fn validate_known_payloads(
        &self,
        limits: &PayloadLimits,
    ) -> Result<(), PayloadValidationError> {
        if matches!(self.header, ContainerHeader::Flat(_)) {
            return Ok(());
        }

        for chunk in self.chunks() {
            match preflight_chunk(chunk, limits) {
                Ok(PreflightStatus::Validated | PreflightStatus::Custom) => {}
                Ok(PreflightStatus::UnsupportedStandard) => {
                    return Err(PayloadValidationError::at_chunk(
                        chunk,
                        PayloadValidationFailure::UnsupportedStandardPayload,
                    ));
                }
                Err(failure) => {
                    return Err(PayloadValidationError::at_chunk(chunk, failure));
                }
            }
        }
        Ok(())
    }

    pub(super) fn validate_critical_payloads(
        &self,
        limits: &PayloadLimits,
    ) -> Result<(), ReadError> {
        for chunk in self.chunks().filter(|chunk| chunk.flags().is_critical()) {
            require_understood_critical(chunk, preflight_chunk(chunk, limits))?;
        }
        Ok(())
    }
}

pub(crate) fn require_understood_critical(
    chunk: ChunkRef<'_>,
    preflight: Result<PreflightStatus, PayloadValidationFailure>,
) -> Result<(), ReadError> {
    match preflight {
        Ok(PreflightStatus::Validated) => Ok(()),
        Ok(PreflightStatus::UnsupportedStandard) => Err(ReadError::CriticalPayload(
            PayloadValidationError::at_chunk(
                chunk,
                PayloadValidationFailure::UnsupportedStandardPayload,
            ),
        )),
        Ok(PreflightStatus::Custom) => Err(ReadError::UnknownCriticalChunk {
            index: u16::try_from(chunk.index()).expect("validated MIRX chunk count"),
            chunk_type: chunk.chunk_type(),
            payload_offset: chunk.payload_offset(),
        }),
        Err(failure) => Err(ReadError::CriticalPayload(
            PayloadValidationError::at_chunk(chunk, failure),
        )),
    }
}

pub(crate) fn preflight_chunk(
    chunk: ChunkRef<'_>,
    limits: &PayloadLimits,
) -> Result<PreflightStatus, PayloadValidationFailure> {
    match chunk.chunk_type() {
        ChunkType::IMAGE => {
            if let Some(ImageRef::Encoded(image)) =
                chunk.image().map_err(PayloadValidationFailure::Image)?
            {
                image.preflight(limits).map_err(|error| {
                    PayloadValidationFailure::Image(ImageReadError::Encoded(error))
                })?;
            }
            Ok(PreflightStatus::Validated)
        }
        ChunkType::FONT => {
            FontView::open_at(chunk.payload(), chunk.payload_offset(), limits)
                .and_then(|font| font.preflight(limits))
                .map_err(PayloadValidationFailure::Font)?;
            Ok(PreflightStatus::Validated)
        }
        ChunkType::VECTOR => {
            Scene::preflight(chunk.payload(), limits).map_err(PayloadValidationFailure::Vector)?;
            Ok(PreflightStatus::Validated)
        }
        ChunkType::META => {
            chunk.meta(limits).map_err(PayloadValidationFailure::Meta)?;
            Ok(PreflightStatus::Validated)
        }
        ChunkType::PALETTE => {
            chunk
                .palette(limits)
                .map_err(PayloadValidationFailure::Palette)?;
            Ok(PreflightStatus::Validated)
        }
        ChunkType::FRAMES => {
            let frames = chunk
                .frames(limits)
                .map_err(PayloadValidationFailure::Frames)?
                .expect("FRAMES chunk must return a typed view");
            frames
                .validate_data()
                .map_err(PayloadValidationFailure::Frames)?;
            Ok(PreflightStatus::Validated)
        }
        chunk_type if is_standard(chunk_type) => Ok(PreflightStatus::UnsupportedStandard),
        _ => Ok(PreflightStatus::Custom),
    }
}

const fn is_standard(chunk_type: ChunkType) -> bool {
    matches!(
        chunk_type,
        ChunkType::IMAGE
            | ChunkType::FRAMES
            | ChunkType::VECTOR
            | ChunkType::FONT
            | ChunkType::META
            | ChunkType::PALETTE
    )
}

#[cfg(test)]
#[path = "../../tests/unit/reader/preflight/tests.rs"]
mod tests;
