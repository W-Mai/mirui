use core::ops::Range;

use super::IntegrityRanges;
use crate::media::{MediaPayload, MediaPayloadError, MediaSectionKind};

/// Metadata-only DATA verification plan with an exact checksum byte count.
///
/// Planning does not establish DATA integrity. Only `verify` scans sample or
/// encoded bytes; neither operation allocates or performs streamed I/O.
#[derive(Clone, Debug)]
pub struct DataCheckPlan<'a> {
    media: MediaPayload<'a>,
    coverage: Coverage<'a>,
    byte_len: u32,
}

#[derive(Clone, Debug)]
enum Coverage<'a> {
    Empty,
    Whole,
    Indexed(IntegrityRanges<'a>),
}

impl<'a> MediaPayload<'a> {
    /// Plans actual checksum coverage for a payload-relative DATA request.
    ///
    /// The request must lie inside one DATA section. Whole-DATA integrity charges
    /// all DATA bodies for any nonempty request. Indexed integrity selects only
    /// intersecting partitions. Empty requests check no DATA bytes. Planning
    /// scans section metadata and uses bounded index lookups, not DATA reads.
    ///
    /// ```
    /// use mirx::{coding::Rle, image::{ColorDescription, EncodedImageAsset, SampleLayout, SurfaceDescriptor},
    ///     media::{DataIntegrity, MediaPayload, MediaSectionKind}};
    /// let surface = SurfaceDescriptor::new(4, 2, SampleLayout::A8, ColorDescription::NONE).unwrap();
    /// let payload = EncodedImageAsset::new(surface, Rle::new().record(), &[0x83, 1, 0x83, 2])
    ///     .with_integrity(DataIntegrity::Indexed(&[2, 4])).encode().unwrap();
    /// let media = MediaPayload::open(&payload).unwrap();
    /// let start = media.section(MediaSectionKind::DATA).unwrap().descriptor().offset();
    /// let plan = media.data_check_plan(start + 1..start + 2).unwrap();
    /// assert_eq!(plan.byte_len(), 2);
    /// plan.verify().unwrap();
    /// ```
    pub fn data_check_plan(
        self,
        requested: Range<u32>,
    ) -> Result<DataCheckPlan<'a>, MediaPayloadError> {
        if requested.start > requested.end
            || !self
                .sections_of_kind(MediaSectionKind::DATA)
                .any(|section| {
                    let descriptor = section.descriptor();
                    requested.start >= descriptor.offset()
                        && requested.end <= descriptor.offset() + descriptor.size()
                })
        {
            return Err(MediaPayloadError::InvalidDataRange);
        }
        let (coverage, byte_len) = if requested.is_empty() {
            (Coverage::Empty, 0)
        } else if let Some(table) = self.integrity() {
            let ranges = table.intersecting(requested);
            let first = ranges.clone().next().expect("validated DATA coverage");
            let last = ranges.clone().next_back().expect("validated DATA coverage");
            // Partitions inside one DATA section have no gaps or overlap.
            let byte_len = last.range().end - first.offset();
            (Coverage::Indexed(ranges), byte_len)
        } else {
            let byte_len = self
                .sections_of_kind(MediaSectionKind::DATA)
                .map(|section| section.descriptor().size())
                .sum();
            (Coverage::Whole, byte_len)
        };
        Ok(DataCheckPlan {
            media: self,
            coverage,
            byte_len,
        })
    }
}

impl DataCheckPlan<'_> {
    /// Exact DATA bytes scanned by `verify`, excluding already-read metadata.
    pub const fn byte_len(&self) -> u32 {
        self.byte_len
    }

    /// Verifies the planned immutable bytes; constructing this plan is not verification.
    pub fn verify(self) -> Result<(), MediaPayloadError> {
        match self.coverage {
            Coverage::Empty => Ok(()),
            Coverage::Whole => self.media.validate_data(),
            Coverage::Indexed(ranges) => {
                for range in ranges {
                    self.media.validate_integrity_range(range)?;
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests;
