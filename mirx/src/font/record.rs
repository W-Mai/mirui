#![doc = include_str!("../../docs/font-representations.md")]

use super::{FontRepresentation, FontRepresentationError, FontRepresentationKind};
use crate::image::{SampleLayout, SurfaceDescriptor, SurfacePlanError, SurfaceRequirements};
use crate::wire::{read_u16_le, read_u32_le, write_u16_le, write_u32_le};

pub const REPRESENTATION_RECORD_LEN: usize = 16;

/// Compact representation semantics and references to shared glyph storage.
///
/// Sample depth and decoded selection cost live only in the bound surface.
/// Native metadata must pass `validate_for` at complete face binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RepresentationRecord {
    metadata: FontRepresentation,
    surface_index: u16,
    glyph_map_offset: u32,
}

impl RepresentationRecord {
    pub const fn new(metadata: FontRepresentation, surface_index: u16) -> Self {
        Self {
            metadata,
            surface_index,
            glyph_map_offset: 0,
        }
    }

    pub const fn with_glyph_map_offset(mut self, offset: u32) -> Self {
        self.glyph_map_offset = offset;
        self
    }

    pub const fn representation(self) -> FontRepresentation {
        self.metadata
    }

    pub const fn surface_index(self) -> u16 {
        self.surface_index
    }

    pub const fn glyph_map_offset(self) -> u32 {
        self.glyph_map_offset
    }

    /// Resolves one record against logical surface metadata, without sample I/O.
    /// Trailing bytes are not consumed; the complete face checks referenced tables.
    pub fn from_record(
        bytes: &[u8],
        surface: SurfaceDescriptor,
    ) -> Result<Self, RepresentationRecordError> {
        if bytes.len() < REPRESENTATION_RECORD_LEN {
            return Err(RepresentationRecordError::Truncated {
                needed: REPRESENTATION_RECORD_LEN,
                available: bytes.len(),
            });
        }
        if bytes[1] != 0 {
            return Err(RepresentationRecordError::ReservedNonZero { offset: 1 });
        }
        let metadata = Fields {
            class: bytes[0],
            design_ppem: read_u16_le(bytes, 2).unwrap(),
            min_ppem: read_u16_le(bytes, 4).unwrap(),
            max_ppem: read_u16_le(bytes, 6).unwrap(),
            detail: read_u16_le(bytes, 8).unwrap(),
        }
        .resolve(surface)?;
        Ok(Self {
            metadata,
            surface_index: read_u16_le(bytes, 10).unwrap(),
            glyph_map_offset: read_u32_le(bytes, 12).unwrap(),
        })
    }

    /// Rejects stale native depth or cost metadata before complete face emission.
    pub fn validate_for(self, surface: SurfaceDescriptor) -> Result<(), RepresentationRecordError> {
        let resolved = Fields::from_metadata(self.metadata).resolve(surface)?;
        match (self.metadata.kind(), resolved.kind()) {
            (
                FontRepresentationKind::Coverage { bits: actual },
                FontRepresentationKind::Coverage { bits: expected },
            )
            | (
                FontRepresentationKind::SignedDistance { bits: actual, .. },
                FontRepresentationKind::SignedDistance { bits: expected, .. },
            ) if actual != expected => {
                return Err(RepresentationRecordError::SampleDepthMismatch { expected, actual });
            }
            _ => {}
        }
        if self.metadata.decoded_bytes() != resolved.decoded_bytes() {
            return Err(RepresentationRecordError::DecodedSizeMismatch {
                expected: resolved.decoded_bytes(),
                actual: self.metadata.decoded_bytes(),
            });
        }
        Ok(())
    }

    /// Emits 16 canonical bytes without serializing derived surface facts.
    /// Capacity errors leave output unchanged; successful writes preserve suffixes.
    pub fn encode_record_into(self, out: &mut [u8]) -> Result<usize, RepresentationRecordError> {
        if out.len() < REPRESENTATION_RECORD_LEN {
            return Err(RepresentationRecordError::BufferTooSmall {
                needed: REPRESENTATION_RECORD_LEN,
                available: out.len(),
            });
        }
        let fields = Fields::from_metadata(self.metadata);
        let mut record = [0; REPRESENTATION_RECORD_LEN];
        record[0] = fields.class;
        write_u16_le(&mut record, 2, fields.design_ppem);
        write_u16_le(&mut record, 4, fields.min_ppem);
        write_u16_le(&mut record, 6, fields.max_ppem);
        write_u16_le(&mut record, 8, fields.detail);
        write_u16_le(&mut record, 10, self.surface_index);
        write_u32_le(&mut record, 12, self.glyph_map_offset);
        out[..REPRESENTATION_RECORD_LEN].copy_from_slice(&record);
        Ok(REPRESENTATION_RECORD_LEN)
    }
}

#[derive(Clone, Copy)]
struct Fields {
    class: u8,
    design_ppem: u16,
    min_ppem: u16,
    max_ppem: u16,
    detail: u16,
}

impl Fields {
    const fn from_metadata(metadata: FontRepresentation) -> Self {
        let (class, min_ppem, max_ppem, detail) = match metadata.kind() {
            FontRepresentationKind::Coverage { .. } => (0, 0, 0, 0),
            FontRepresentationKind::SignedDistance { spread, .. } => {
                (1, metadata.min_ppem(), metadata.max_ppem(), spread)
            }
            FontRepresentationKind::Application(kind) => {
                (2, metadata.min_ppem(), metadata.max_ppem(), kind)
            }
        };
        Self {
            class,
            design_ppem: metadata.design_ppem(),
            min_ppem,
            max_ppem,
            detail,
        }
    }

    fn resolve(
        self,
        surface: SurfaceDescriptor,
    ) -> Result<FontRepresentation, RepresentationRecordError> {
        if self.class > 2 {
            return Err(RepresentationRecordError::UnknownClass(self.class));
        }
        if self.class == 0 && (self.min_ppem != 0 || self.max_ppem != 0 || self.detail != 0) {
            return Err(RepresentationRecordError::NonCanonicalCoverage);
        }
        let layout = surface.sample_layout();
        let bits = if self.class < 2 {
            if !layout.is_alpha() {
                return Err(RepresentationRecordError::UnsupportedLayout(layout));
            }
            surface.plane(0).expect("scalar plane").bits_per_element()
        } else {
            0
        };
        let decoded_bytes = surface
            .memory_plan(SurfaceRequirements::new())
            .map_err(RepresentationRecordError::Memory)?
            .byte_len();
        match self.class {
            0 => FontRepresentation::coverage(bits, self.design_ppem, decoded_bytes),
            1 => FontRepresentation::signed_distance(
                bits,
                self.detail,
                self.design_ppem,
                self.min_ppem,
                self.max_ppem,
                decoded_bytes,
            ),
            2 => FontRepresentation::application(
                self.detail,
                self.design_ppem,
                self.min_ppem,
                self.max_ppem,
                decoded_bytes,
            ),
            _ => unreachable!("validated representation class"),
        }
        .map_err(RepresentationRecordError::Representation)
    }
}

/// Invalid wire semantics, inconsistent native surface hints or short output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RepresentationRecordError {
    Truncated { needed: usize, available: usize },
    BufferTooSmall { needed: usize, available: usize },
    ReservedNonZero { offset: usize },
    UnknownClass(u8),
    NonCanonicalCoverage,
    UnsupportedLayout(SampleLayout),
    Memory(SurfacePlanError),
    Representation(FontRepresentationError),
    SampleDepthMismatch { expected: u8, actual: u8 },
    DecodedSizeMismatch { expected: u32, actual: u32 },
}

#[cfg(test)]
mod tests;
