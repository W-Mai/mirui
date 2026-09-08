use super::{BlendMode, DisposalMode, FrameSequence, FrameSequenceError};
use crate::{
    image::{Region, RegionError, SurfaceDescriptor},
    wire::{read_u32_le, write_u32_le},
};

pub(crate) const FRAME_COMPOSITION_RECORD_LEN: usize = 24;
const HAS_REGION: u8 = 1;

/// Resolved composition state for one presentation frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameComposition {
    region: Option<Region>,
    blend: BlendMode,
    disposal: DisposalMode,
}

impl FrameComposition {
    /// Uses the selected unit regions without storing another rectangle.
    pub const fn new(blend: BlendMode, disposal: DisposalMode) -> Self {
        Self {
            region: None,
            blend,
            disposal,
        }
    }

    /// Overrides the region affected by post-presentation disposal.
    pub const fn with_region(mut self, region: Region) -> Self {
        self.region = Some(region);
        self
    }

    /// None means the exact regions selected by the frame's groups.
    pub const fn region(self) -> Option<Region> {
        self.region
    }

    pub const fn blend(self) -> BlendMode {
        self.blend
    }

    pub const fn disposal(self) -> DisposalMode {
        self.disposal
    }
}

/// One native sparse override before canonical wire emission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameCompositionOverride {
    frame: u32,
    composition: FrameComposition,
}

impl FrameCompositionOverride {
    pub const fn new(frame: u32, composition: FrameComposition) -> Self {
        Self { frame, composition }
    }

    pub const fn frame(self) -> u32 {
        self.frame
    }

    pub const fn composition(self) -> FrameComposition {
        self.composition
    }

    pub(crate) fn encode_record(self) -> [u8; FRAME_COMPOSITION_RECORD_LEN] {
        let mut bytes = [0; FRAME_COMPOSITION_RECORD_LEN];
        write_u32_le(&mut bytes, 0, self.frame);
        if let Some(region) = self.composition.region {
            write_u32_le(&mut bytes, 4, region.x());
            write_u32_le(&mut bytes, 8, region.y());
            write_u32_le(&mut bytes, 12, region.width());
            write_u32_le(&mut bytes, 16, region.height());
            bytes[22] = HAS_REGION;
        }
        bytes[20] = self.composition.blend as u8;
        bytes[21] = self.composition.disposal as u8;
        bytes
    }

    fn open(bytes: &[u8]) -> Result<Self, FrameCompositionError> {
        let frame = read_u32_le(bytes, 0).expect("complete composition record");
        let flags = bytes[22];
        if flags & !HAS_REGION != 0 {
            return Err(FrameCompositionError::UnknownFlags { frame, flags });
        }
        if bytes[23] != 0 {
            return Err(FrameCompositionError::NonCanonicalReserved { frame });
        }
        let blend = BlendMode::open(bytes[20]).map_err(|error| map_sequence(frame, error))?;
        let disposal = DisposalMode::open(bytes[21]).map_err(|error| map_sequence(frame, error))?;
        let fields = (
            read_u32_le(bytes, 4).expect("complete composition record"),
            read_u32_le(bytes, 8).expect("complete composition record"),
            read_u32_le(bytes, 12).expect("complete composition record"),
            read_u32_le(bytes, 16).expect("complete composition record"),
        );
        let region = if flags & HAS_REGION == 0 {
            if fields != (0, 0, 0, 0) {
                return Err(FrameCompositionError::NonCanonicalRegion { frame });
            }
            None
        } else {
            Some(
                Region::new(fields.0, fields.1, fields.2, fields.3)
                    .map_err(|error| FrameCompositionError::Region { frame, error })?,
            )
        };
        Ok(Self::new(
            frame,
            FrameComposition {
                region,
                blend,
                disposal,
            },
        ))
    }
}

/// Borrowed sparse composition overrides with default lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameCompositionTable<'a> {
    bytes: &'a [u8],
    sequence: FrameSequence,
}

impl<'a> FrameCompositionTable<'a> {
    pub fn open(
        bytes: &'a [u8],
        sequence: FrameSequence,
        surface: SurfaceDescriptor,
    ) -> Result<Self, FrameCompositionError> {
        if bytes.is_empty() || bytes.len() % FRAME_COMPOSITION_RECORD_LEN != 0 {
            return Err(FrameCompositionError::InvalidLength(bytes.len()));
        }
        let mut previous = None;
        for record in bytes.chunks_exact(FRAME_COMPOSITION_RECORD_LEN) {
            let record = FrameCompositionOverride::open(record)?;
            validate_record(record, sequence, surface, previous)?;
            previous = Some(record.frame);
        }
        Ok(Self { bytes, sequence })
    }

    pub const fn len(self) -> usize {
        self.bytes.len() / FRAME_COMPOSITION_RECORD_LEN
    }

    pub const fn is_empty(self) -> bool {
        false
    }

    /// Returns sequence defaults for frames without a stored override.
    pub fn get(self, frame: u32) -> Option<FrameComposition> {
        if frame >= self.sequence.frame_count() {
            return None;
        }
        let mut left = 0usize;
        let mut right = self.len();
        while left < right {
            let middle = left + (right - left) / 2;
            let record = FrameCompositionOverride::open(
                &self.bytes[middle * FRAME_COMPOSITION_RECORD_LEN..]
                    [..FRAME_COMPOSITION_RECORD_LEN],
            )
            .expect("validated composition table");
            match record.frame.cmp(&frame) {
                core::cmp::Ordering::Less => left = middle + 1,
                core::cmp::Ordering::Greater => right = middle,
                core::cmp::Ordering::Equal => return Some(record.composition),
            }
        }
        Some(FrameComposition::new(
            self.sequence.default_blend(),
            self.sequence.default_disposal(),
        ))
    }
}

/// Validated native sparse composition overrides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FrameCompositionAsset<'a> {
    records: &'a [FrameCompositionOverride],
}

impl<'a> FrameCompositionAsset<'a> {
    pub(super) fn new(
        records: &'a [FrameCompositionOverride],
        sequence: FrameSequence,
        surface: SurfaceDescriptor,
    ) -> Result<Self, FrameCompositionError> {
        if records.is_empty() {
            return Err(FrameCompositionError::InvalidLength(0));
        }
        let mut previous = None;
        for &record in records {
            validate_record(record, sequence, surface, previous)?;
            previous = Some(record.frame);
        }
        records
            .len()
            .checked_mul(FRAME_COMPOSITION_RECORD_LEN)
            .ok_or(FrameCompositionError::SizeOverflow)?;
        Ok(Self { records })
    }

    pub(super) const fn encoded_len(self) -> usize {
        self.records.len() * FRAME_COMPOSITION_RECORD_LEN
    }

    pub(crate) const fn records(self) -> &'a [FrameCompositionOverride] {
        self.records
    }

    /// Errors preserve the complete output buffer.
    #[cfg(test)]
    pub(super) fn encode_into(self, output: &mut [u8]) -> Result<usize, FrameCompositionError> {
        let needed = self.encoded_len();
        if output.len() < needed {
            return Err(FrameCompositionError::BufferTooSmall {
                needed,
                available: output.len(),
            });
        }
        for (index, record) in self.records.iter().copied().enumerate() {
            let start = index * FRAME_COMPOSITION_RECORD_LEN;
            output[start..start + FRAME_COMPOSITION_RECORD_LEN]
                .copy_from_slice(&record.encode_record());
        }
        Ok(needed)
    }
}

fn validate_record(
    record: FrameCompositionOverride,
    sequence: FrameSequence,
    surface: SurfaceDescriptor,
    previous: Option<u32>,
) -> Result<(), FrameCompositionError> {
    if record.frame >= sequence.frame_count() {
        return Err(FrameCompositionError::FrameOutOfBounds {
            frame: record.frame,
            frame_count: sequence.frame_count(),
        });
    }
    if previous.is_some_and(|previous| record.frame <= previous) {
        return Err(FrameCompositionError::FramesOutOfOrder {
            previous: previous.unwrap(),
            next: record.frame,
        });
    }
    if record.composition.region.is_none()
        && record.composition.blend == sequence.default_blend()
        && record.composition.disposal == sequence.default_disposal()
    {
        return Err(FrameCompositionError::RedundantDefault {
            frame: record.frame,
        });
    }
    if let Some(region) = record.composition.region {
        if region.is_empty() {
            return Err(FrameCompositionError::EmptyRegion {
                frame: record.frame,
            });
        }
        surface
            .region(region.x(), region.y(), region.width(), region.height())
            .map_err(|error| FrameCompositionError::Region {
                frame: record.frame,
                error,
            })?;
        for plane in 0..surface.plane_count() {
            region
                .for_plane(surface, plane)
                .map_err(|error| FrameCompositionError::Region {
                    frame: record.frame,
                    error,
                })?;
        }
    }
    Ok(())
}

fn map_sequence(frame: u32, error: FrameSequenceError) -> FrameCompositionError {
    match error {
        FrameSequenceError::UnknownBlendMode(value) => {
            FrameCompositionError::UnknownBlendMode { frame, value }
        }
        FrameSequenceError::UnknownDisposalMode(value) => {
            FrameCompositionError::UnknownDisposalMode { frame, value }
        }
        _ => unreachable!("composition only parses sequence enums"),
    }
}

/// Failure while reading or emitting sparse frame composition overrides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameCompositionError {
    InvalidLength(usize),
    FrameOutOfBounds { frame: u32, frame_count: u32 },
    FramesOutOfOrder { previous: u32, next: u32 },
    UnknownBlendMode { frame: u32, value: u8 },
    UnknownDisposalMode { frame: u32, value: u8 },
    UnknownFlags { frame: u32, flags: u8 },
    NonCanonicalReserved { frame: u32 },
    NonCanonicalRegion { frame: u32 },
    RedundantDefault { frame: u32 },
    EmptyRegion { frame: u32 },
    Region { frame: u32, error: RegionError },
    BufferTooSmall { needed: usize, available: usize },
    SizeOverflow,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::{ColorDescription, SampleLayout};

    fn surface() -> SurfaceDescriptor {
        SurfaceDescriptor::new(16, 12, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap()
    }

    fn sequence() -> FrameSequence {
        FrameSequence::new(5, 1_000, 40).unwrap()
    }

    #[test]
    fn sparse_overrides_roundtrip_and_missing_frames_use_defaults() {
        let region = surface().region(4, 2, 8, 6).unwrap();
        let records = [
            FrameCompositionOverride::new(
                1,
                FrameComposition::new(BlendMode::SourceOver, DisposalMode::Keep),
            ),
            FrameCompositionOverride::new(
                4,
                FrameComposition::new(BlendMode::Replace, DisposalMode::Clear).with_region(region),
            ),
        ];
        let asset = FrameCompositionAsset::new(&records, sequence(), surface()).unwrap();
        let mut bytes = [0; FRAME_COMPOSITION_RECORD_LEN * 2];
        asset.encode_into(&mut bytes).unwrap();
        let table = FrameCompositionTable::open(&bytes, sequence(), surface()).unwrap();
        assert_eq!(
            table.get(0),
            Some(FrameComposition::new(
                BlendMode::Replace,
                DisposalMode::Keep
            ))
        );
        assert_eq!(table.get(1), Some(records[0].composition()));
        assert_eq!(table.get(4), Some(records[1].composition()));
        assert_eq!(table.get(5), None);
    }

    #[test]
    fn redundant_out_of_order_and_out_of_bounds_records_are_rejected() {
        let default = FrameCompositionOverride::new(
            1,
            FrameComposition::new(BlendMode::Replace, DisposalMode::Keep),
        );
        assert_eq!(
            FrameCompositionAsset::new(&[default], sequence(), surface()),
            Err(FrameCompositionError::RedundantDefault { frame: 1 })
        );
        let changed = FrameComposition::new(BlendMode::SourceOver, DisposalMode::Keep);
        assert_eq!(
            FrameCompositionAsset::new(
                &[
                    FrameCompositionOverride::new(2, changed),
                    FrameCompositionOverride::new(1, changed),
                ],
                sequence(),
                surface(),
            ),
            Err(FrameCompositionError::FramesOutOfOrder {
                previous: 2,
                next: 1
            })
        );
        assert_eq!(
            FrameCompositionAsset::new(
                &[FrameCompositionOverride::new(5, changed)],
                sequence(),
                surface(),
            ),
            Err(FrameCompositionError::FrameOutOfBounds {
                frame: 5,
                frame_count: 5
            })
        );
    }

    #[test]
    fn explicit_regions_must_be_nonempty_and_inside_the_surface() {
        let empty = Region::new(0, 0, 0, 1).unwrap();
        let record = FrameCompositionOverride::new(
            2,
            FrameComposition::new(BlendMode::Replace, DisposalMode::Clear).with_region(empty),
        );
        assert_eq!(
            FrameCompositionAsset::new(&[record], sequence(), surface()),
            Err(FrameCompositionError::EmptyRegion { frame: 2 })
        );

        let outside = Region::new(15, 0, 2, 1).unwrap();
        let record = FrameCompositionOverride::new(
            2,
            FrameComposition::new(BlendMode::Replace, DisposalMode::Clear).with_region(outside),
        );
        assert_eq!(
            FrameCompositionAsset::new(&[record], sequence(), surface()),
            Err(FrameCompositionError::Region {
                frame: 2,
                error: RegionError::OutOfBounds
            })
        );
    }

    #[test]
    fn explicit_yuv_regions_cannot_split_chroma_samples() {
        let surface = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let record = [FrameCompositionOverride::new(
            1,
            FrameComposition::new(BlendMode::Replace, DisposalMode::Clear)
                .with_region(surface.region(1, 0, 2, 2).unwrap()),
        )];
        assert!(matches!(
            FrameCompositionAsset::new(&record, sequence(), surface),
            Err(FrameCompositionError::Region {
                frame: 1,
                error: RegionError::UnalignedPlaneRegion { index: 1, .. },
            })
        ));
    }

    #[test]
    fn malformed_flags_reserved_fields_and_capacity_are_rejected() {
        let record = FrameCompositionOverride::new(
            1,
            FrameComposition::new(BlendMode::SourceOver, DisposalMode::Keep),
        );
        let records = [record];
        let asset = FrameCompositionAsset::new(&records, sequence(), surface()).unwrap();
        let mut bytes = [0; FRAME_COMPOSITION_RECORD_LEN];
        asset.encode_into(&mut bytes).unwrap();

        let mut unknown = bytes;
        unknown[22] = 2;
        assert_eq!(
            FrameCompositionTable::open(&unknown, sequence(), surface()),
            Err(FrameCompositionError::UnknownFlags { frame: 1, flags: 2 })
        );
        unknown = bytes;
        unknown[23] = 1;
        assert_eq!(
            FrameCompositionTable::open(&unknown, sequence(), surface()),
            Err(FrameCompositionError::NonCanonicalReserved { frame: 1 })
        );
        let mut short = [0xa5; FRAME_COMPOSITION_RECORD_LEN - 1];
        assert_eq!(
            asset.encode_into(&mut short),
            Err(FrameCompositionError::BufferTooSmall {
                needed: FRAME_COMPOSITION_RECORD_LEN,
                available: FRAME_COMPOSITION_RECORD_LEN - 1,
            })
        );
        assert_eq!(short, [0xa5; FRAME_COMPOSITION_RECORD_LEN - 1]);
    }
}
