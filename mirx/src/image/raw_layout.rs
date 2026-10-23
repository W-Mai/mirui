use super::{
    PLANE_RECORD_LEN, PlaneGeometry, PlaneMemoryError, PlaneMemoryLayout, PlaneMemoryRecordError,
    RawImageViewError, SURFACE_RECORD_LEN, SurfaceDescriptor,
};
use crate::media::MediaSectionKind;

/// Checked RAW plane geometry without retaining DATA bytes.
///
/// Only the optional PLANES records are borrowed. The supplied surface must come from a checked SURFACE record. Directory rules, color tables and checksums require separate validation before publishing an image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawImageLayout<'a> {
    surface: SurfaceDescriptor,
    records: Option<&'a [u8]>,
}

impl<'a> RawImageLayout<'a> {
    /// Parse isolated SURFACE and optional PLANES windows. Their exact lengths are checked; DATA bytes are neither borrowed nor read.
    pub fn parse(
        surface_record: &[u8],
        records: Option<&'a [u8]>,
        data_len: u32,
        data_file_offset: Option<u32>,
    ) -> Result<Self, RawImageViewError> {
        Self::new(
            parse_surface(surface_record)?,
            records,
            data_len,
            data_file_offset,
        )
    }

    /// Validate every plane against the declared DATA length and optional absolute DATA offset. No DATA storage is required or allocated.
    pub fn new(
        surface: SurfaceDescriptor,
        records: Option<&'a [u8]>,
        data_len: u32,
        data_file_offset: Option<u32>,
    ) -> Result<Self, RawImageViewError> {
        if let Some(records) = records {
            let expected = usize::from(surface.plane_count()) * PLANE_RECORD_LEN;
            if records.len() != expected {
                return Err(RawImageViewError::SectionSizeMismatch {
                    kind: MediaSectionKind::PLANES,
                    expected,
                    actual: records.len(),
                });
            }
        }
        let layout = Self { surface, records };
        let mut previous_end = 0;
        for index in 0..surface.plane_count() {
            let geometry = surface.plane(index).expect("validated surface plane index");
            let memory = layout.plane_memory(index, geometry)?;
            if memory.data_offset() < previous_end {
                return Err(RawImageViewError::PlaneRangesOverlap {
                    previous: index - 1,
                    next: index,
                });
            }
            if memory.data_end() > data_len {
                return Err(RawImageViewError::PlaneDataOutOfBounds {
                    index,
                    offset: memory.data_offset(),
                    size: memory.byte_len(),
                    data_size: data_len,
                });
            }
            if let Some(data_offset) = data_file_offset {
                if !memory.file_address_is_aligned(data_offset) {
                    let absolute_offset = data_offset
                        .checked_add(memory.data_offset())
                        .ok_or(RawImageViewError::SizeOverflow)?;
                    return Err(RawImageViewError::PlaneFileAddressUnaligned {
                        index,
                        absolute_offset,
                        alignment: memory.required_alignment(),
                    });
                }
            }
            previous_end = memory.data_end();
        }
        if records.is_none() && previous_end != data_len {
            return Err(RawImageViewError::CanonicalDataSizeMismatch {
                expected: previous_end,
                actual: data_len,
            });
        }
        Ok(layout)
    }

    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }

    pub fn plane(self, index: u8) -> Option<PlaneMemoryLayout> {
        let geometry = self.surface.plane(index)?;
        Some(
            self.plane_memory(index, geometry)
                .expect("validated RAW layout"),
        )
    }

    pub(super) fn plane_memory(
        self,
        index: u8,
        geometry: PlaneGeometry,
    ) -> Result<PlaneMemoryLayout, RawImageViewError> {
        plane_memory(self.surface, self.records, index, geometry)
    }
}

pub(super) fn parse_surface(record: &[u8]) -> Result<SurfaceDescriptor, RawImageViewError> {
    if record.len() != SURFACE_RECORD_LEN {
        return Err(RawImageViewError::SectionSizeMismatch {
            kind: MediaSectionKind::SURFACE,
            expected: SURFACE_RECORD_LEN,
            actual: record.len(),
        });
    }
    SurfaceDescriptor::from_record(record).map_err(RawImageViewError::Surface)
}

pub(super) fn plane_memory(
    surface: SurfaceDescriptor,
    records: Option<&[u8]>,
    index: u8,
    geometry: PlaneGeometry,
) -> Result<PlaneMemoryLayout, RawImageViewError> {
    match records {
        Some(records) => {
            let start = usize::from(index) * PLANE_RECORD_LEN;
            let record = records
                .get(start..start + PLANE_RECORD_LEN)
                .ok_or(RawImageViewError::SizeOverflow)?;
            PlaneMemoryLayout::from_record(geometry, record)
                .map_err(|error| RawImageViewError::InvalidPlaneRecord { index, error })
        }
        None => canonical_plane_memory(surface, index).map_err(|error| {
            RawImageViewError::InvalidPlaneRecord {
                index,
                error: PlaneMemoryRecordError::InvalidLayout(error),
            }
        }),
    }
}

fn canonical_plane_memory(
    surface: SurfaceDescriptor,
    requested: u8,
) -> Result<PlaneMemoryLayout, PlaneMemoryError> {
    let mut offset = 0;
    for index in 0..surface.plane_count() {
        let geometry = surface.plane(index).ok_or(PlaneMemoryError::SizeOverflow)?;
        let memory = PlaneMemoryLayout::builder(geometry)
            .with_data_offset(offset)
            .build()?;
        if index == requested {
            return Ok(memory);
        }
        offset = memory.data_end();
    }
    Err(PlaneMemoryError::SizeOverflow)
}
