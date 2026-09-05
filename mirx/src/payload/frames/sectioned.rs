use core::ops::Range;

use super::{
    FRAME_SEQUENCE_RECORD_LEN, FrameComposition, FrameCompositionError, FrameCompositionTable,
    FrameMap, FrameMapError, FrameSequence, FrameSequenceError, FrameTiming, FrameTimingError,
    KeyframeIndex, KeyframeIndexError,
};
use crate::{
    PayloadLimits,
    image::{
        CodingRecords, ColorTableError, CoverageBudget, EncodedImageError, GroupRecords,
        GroupSource, ReferenceMode, SURFACE_RECORD_LEN, SurfaceDescriptor, SurfaceRecordError,
        UNIT_GROUP_RECORD_LEN,
    },
    media::{
        CodingTable, CodingTableError, MediaPayload, MediaPayloadError, MediaSection,
        MediaSectionKind,
    },
    payload::ColorTableView,
};

/// Sectioned FRAMES metadata without decoded samples or a materialized group table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SectionedFramesView<'a> {
    media: MediaPayload<'a>,
    sequence: FrameSequence,
    surface: SurfaceDescriptor,
    codings: CodingTable<'a>,
    groups: MediaSection<'a>,
    indexes: Option<MediaSection<'a>>,
    map: FrameMap<'a>,
    timing: Option<FrameTiming<'a>>,
    composition: Option<FrameCompositionTable<'a>>,
    keyframes: Option<KeyframeIndex<'a>>,
    data: MediaSection<'a>,
    color_table: Option<ColorTableView<'a>>,
    file_offset: Option<u32>,
}

/// Resolved presentation defaults and group range for one frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FramePresentation {
    index: u32,
    groups: Range<u32>,
    duration_ticks: u32,
    composition: FrameComposition,
}

impl<'a> SectionedFramesView<'a> {
    /// Opens metadata without decoding or scanning DATA bytes.
    pub fn open(payload: &'a [u8], limits: &PayloadLimits) -> Result<Self, SectionedFramesError> {
        let media = MediaPayload::open(payload).map_err(SectionedFramesError::Media)?;
        Self::from_media(media, None, limits)
    }

    /// Retains the outer payload position for file-relative alignment checks.
    pub fn open_at(
        payload: &'a [u8],
        file_offset: u32,
        limits: &PayloadLimits,
    ) -> Result<Self, SectionedFramesError> {
        let media = MediaPayload::open(payload).map_err(SectionedFramesError::Media)?;
        Self::from_media(media, Some(file_offset), limits)
    }

    fn from_media(
        media: MediaPayload<'a>,
        file_offset: Option<u32>,
        limits: &PayloadLimits,
    ) -> Result<Self, SectionedFramesError> {
        let mut sequence = None;
        let mut surface = None;
        let mut codings = None;
        let mut groups = None;
        let mut indexes = None;
        let mut map = None;
        let mut timing = None;
        let mut composition = None;
        let mut keyframes = None;
        let mut data = None;
        let mut color_table = None;
        for section in media.sections() {
            let kind = section.descriptor().kind();
            let slot = match kind {
                MediaSectionKind::SEQUENCE => Some(&mut sequence),
                MediaSectionKind::SURFACE => Some(&mut surface),
                MediaSectionKind::CODINGS => Some(&mut codings),
                MediaSectionKind::UNIT_GROUPS => Some(&mut groups),
                MediaSectionKind::UNIT_INDEX => Some(&mut indexes),
                MediaSectionKind::FRAME_MAP => Some(&mut map),
                MediaSectionKind::FRAME_TIMING => Some(&mut timing),
                MediaSectionKind::FRAME_COMPOSITION => Some(&mut composition),
                MediaSectionKind::KEYFRAME_INDEX => Some(&mut keyframes),
                MediaSectionKind::DATA => Some(&mut data),
                MediaSectionKind::COLOR_TABLE => Some(&mut color_table),
                MediaSectionKind::INTEGRITY => None,
                MediaSectionKind::PLANES => {
                    return Err(SectionedFramesError::UnexpectedSection(kind));
                }
                _ if section.descriptor().flags().is_required() => {
                    return Err(SectionedFramesError::UnknownRequiredSection(kind));
                }
                _ => None,
            };
            if let Some(slot) = slot {
                if slot.replace(section).is_some() {
                    return Err(SectionedFramesError::DuplicateSection(kind));
                }
                if !section.descriptor().flags().is_required() {
                    return Err(SectionedFramesError::SectionMustBeRequired(kind));
                }
            }
        }
        let sequence = required(sequence, MediaSectionKind::SEQUENCE)?;
        require_size(sequence, FRAME_SEQUENCE_RECORD_LEN)?;
        let sequence =
            FrameSequence::open(sequence.bytes()).map_err(SectionedFramesError::Sequence)?;
        if sequence.frame_count() > limits.max_frame_records() {
            return Err(SectionedFramesError::TooManyFrames {
                count: sequence.frame_count(),
                limit: limits.max_frame_records(),
            });
        }

        let surface_section = required(surface, MediaSectionKind::SURFACE)?;
        require_size(surface_section, SURFACE_RECORD_LEN)?;
        let surface = SurfaceDescriptor::from_record(surface_section.bytes())
            .map_err(SectionedFramesError::Surface)?;
        let codings = CodingTable::open(required(codings, MediaSectionKind::CODINGS)?.bytes())
            .map_err(SectionedFramesError::Codings)?;
        let groups = required(groups, MediaSectionKind::UNIT_GROUPS)?;
        if groups.bytes().is_empty() || groups.bytes().len() % UNIT_GROUP_RECORD_LEN != 0 {
            return Err(SectionedFramesError::InvalidGroupTableLength(
                groups.bytes().len(),
            ));
        }
        let group_count = groups.bytes().len() / UNIT_GROUP_RECORD_LEN;
        if group_count as u64 > u64::from(limits.max_raster_groups()) {
            return Err(SectionedFramesError::TooManyGroups {
                count: group_count,
                limit: limits.max_raster_groups(),
            });
        }
        let map = FrameMap::open(
            required(map, MediaSectionKind::FRAME_MAP)?.bytes(),
            sequence.frame_count(),
        )
        .map_err(SectionedFramesError::Map)?;
        if map.group_count() as usize != group_count {
            return Err(SectionedFramesError::GroupCountMismatch {
                mapped: map.group_count(),
                stored: group_count,
            });
        }
        let timing = timing
            .map(|section| {
                FrameTiming::open(
                    section.bytes(),
                    sequence.frame_count(),
                    sequence.default_duration_ticks(),
                )
            })
            .transpose()
            .map_err(SectionedFramesError::Timing)?;
        let composition = composition
            .map(|section| FrameCompositionTable::open(section.bytes(), sequence, surface))
            .transpose()
            .map_err(SectionedFramesError::Composition)?;
        let keyframes = keyframes
            .map(|section| KeyframeIndex::open(section.bytes(), sequence))
            .transpose()
            .map_err(SectionedFramesError::Keyframes)?;
        let data = required(data, MediaSectionKind::DATA)?;
        let color_table = surface
            .read_color_table(color_table.map(MediaSection::bytes))
            .map_err(map_color_table)?;
        let view = Self {
            media,
            sequence,
            surface,
            codings,
            groups,
            indexes,
            map,
            timing,
            composition,
            keyframes,
            data,
            color_table,
            file_offset,
        };
        let mut budget = CoverageBudget::new(limits.max_raster_work());
        view.group_source()
            .visit_groups_with_references(Some(&mut budget), |_, _| {})
            .map_err(SectionedFramesError::Storage)?;
        view.validate_reference_runs(&mut budget)?;
        Ok(view)
    }

    pub const fn media(self) -> MediaPayload<'a> {
        self.media
    }

    pub const fn sequence(self) -> FrameSequence {
        self.sequence
    }

    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }

    pub const fn codings(self) -> CodingTable<'a> {
        self.codings
    }

    pub const fn frame_map(self) -> FrameMap<'a> {
        self.map
    }

    pub const fn timing(self) -> Option<FrameTiming<'a>> {
        self.timing
    }

    pub const fn compositions(self) -> Option<FrameCompositionTable<'a>> {
        self.composition
    }

    pub const fn keyframes(self) -> Option<KeyframeIndex<'a>> {
        self.keyframes
    }

    pub const fn color_table(self) -> Option<ColorTableView<'a>> {
        self.color_table
    }

    pub const fn group_count(self) -> usize {
        self.groups.bytes().len() / UNIT_GROUP_RECORD_LEN
    }

    pub fn frame(self, index: u32) -> Option<FramePresentation> {
        let groups = self.map.get(index)?;
        let duration_ticks = self.timing.map_or_else(
            || Some(self.sequence.default_duration_ticks()),
            |timing| timing.duration(index),
        )?;
        let composition = self.composition.map_or_else(
            || {
                Some(FrameComposition::new(
                    self.sequence.default_blend(),
                    self.sequence.default_disposal(),
                ))
            },
            |composition| composition.get(index),
        )?;
        Some(FramePresentation {
            index,
            groups,
            duration_ticks,
            composition,
        })
    }

    pub fn validate_data(self) -> Result<(), SectionedFramesError> {
        self.media
            .validate_data()
            .map_err(SectionedFramesError::Media)
    }

    fn group_source(self) -> GroupSource<'a> {
        GroupSource {
            surface: self.surface,
            codings: CodingRecords::Wire(self.codings),
            records: GroupRecords::Wire(self.groups.bytes()),
            data: self.data.bytes(),
            indexes: self.indexes.map_or(&[], MediaSection::bytes),
            file_offset: self.file_offset,
            data_offset: self.data.descriptor().offset(),
        }
    }

    fn validate_reference_runs(
        self,
        budget: &mut CoverageBudget,
    ) -> Result<(), SectionedFramesError> {
        let source = self.group_source();
        let mut delta_frames = 0u32;
        for frame in 0..self.sequence.frame_count() {
            let range = self.map.get(frame).expect("validated frame map");
            let reference = if range.is_empty() {
                ReferenceMode::Previous
            } else {
                let first = source
                    .record(range.start as usize)
                    .map_err(SectionedFramesError::Storage)?
                    .reference();
                for group in range.start + 1..range.end {
                    let next = source
                        .record(group as usize)
                        .map_err(SectionedFramesError::Storage)?
                        .reference();
                    if next != first {
                        return Err(SectionedFramesError::MixedReferences { frame });
                    }
                }
                first
            };
            match reference {
                ReferenceMode::Independent => {
                    source
                        .validate_group_range(range.start as usize..range.end as usize, budget)
                        .map_err(SectionedFramesError::Storage)?;
                    delta_frames = 0;
                }
                ReferenceMode::Previous => {
                    if frame == 0 {
                        return Err(SectionedFramesError::FirstFrameDependsOnPrevious);
                    }
                    delta_frames += 1;
                    if delta_frames > u32::from(self.sequence.max_delta_frames()) {
                        return Err(SectionedFramesError::DeltaBoundExceeded {
                            frame,
                            delta_frames,
                            limit: self.sequence.max_delta_frames(),
                        });
                    }
                }
            }
        }
        if let Some(keyframes) = self.keyframes {
            for frame in keyframes {
                let range = self.map.get(frame).expect("validated keyframe ordinal");
                if range.is_empty()
                    || source
                        .record(range.start as usize)
                        .map_err(SectionedFramesError::Storage)?
                        .reference()
                        != ReferenceMode::Independent
                {
                    return Err(SectionedFramesError::IndexedFrameDependsOnPrevious { frame });
                }
            }
        }
        Ok(())
    }
}

impl FramePresentation {
    pub const fn index(&self) -> u32 {
        self.index
    }

    pub fn groups(&self) -> Range<u32> {
        self.groups.clone()
    }

    pub const fn duration_ticks(&self) -> u32 {
        self.duration_ticks
    }

    pub const fn composition(&self) -> FrameComposition {
        self.composition
    }

    pub fn is_noop(&self) -> bool {
        self.groups.is_empty()
    }
}

fn required<'a>(
    section: Option<MediaSection<'a>>,
    kind: MediaSectionKind,
) -> Result<MediaSection<'a>, SectionedFramesError> {
    section.ok_or(SectionedFramesError::MissingSection(kind))
}

fn require_size(section: MediaSection<'_>, expected: usize) -> Result<(), SectionedFramesError> {
    if section.bytes().len() != expected {
        return Err(SectionedFramesError::SectionSizeMismatch {
            kind: section.descriptor().kind(),
            expected,
            actual: section.bytes().len(),
        });
    }
    Ok(())
}

fn map_color_table(error: ColorTableError) -> SectionedFramesError {
    match error {
        ColorTableError::Missing => SectionedFramesError::MissingColorTable,
        ColorTableError::Unexpected => SectionedFramesError::UnexpectedColorTable,
        ColorTableError::SizeMismatch { expected, actual } => {
            SectionedFramesError::SectionSizeMismatch {
                kind: MediaSectionKind::COLOR_TABLE,
                expected,
                actual,
            }
        }
        ColorTableError::SizeOverflow => SectionedFramesError::SizeOverflow,
    }
}

/// Failure while opening sectioned FRAMES metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SectionedFramesError {
    Media(MediaPayloadError),
    MissingSection(MediaSectionKind),
    DuplicateSection(MediaSectionKind),
    SectionMustBeRequired(MediaSectionKind),
    UnexpectedSection(MediaSectionKind),
    UnknownRequiredSection(MediaSectionKind),
    SectionSizeMismatch {
        kind: MediaSectionKind,
        expected: usize,
        actual: usize,
    },
    Sequence(FrameSequenceError),
    TooManyFrames {
        count: u32,
        limit: u32,
    },
    Surface(SurfaceRecordError),
    Codings(CodingTableError),
    InvalidGroupTableLength(usize),
    TooManyGroups {
        count: usize,
        limit: u32,
    },
    Map(FrameMapError),
    GroupCountMismatch {
        mapped: u32,
        stored: usize,
    },
    Timing(FrameTimingError),
    Composition(FrameCompositionError),
    Keyframes(KeyframeIndexError),
    MissingColorTable,
    UnexpectedColorTable,
    Storage(EncodedImageError),
    FirstFrameDependsOnPrevious,
    MixedReferences {
        frame: u32,
    },
    DeltaBoundExceeded {
        frame: u32,
        delta_frames: u32,
        limit: u16,
    },
    IndexedFrameDependsOnPrevious {
        frame: u32,
    },
    SizeOverflow,
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::*;
    use crate::{
        BlendMode, DisposalMode, FrameCompositionAsset, FrameCompositionOverride, FrameMapAsset,
        FrameTimingAsset, KeyframeIndexAsset,
        image::{ColorDescription, CoverageError, SampleLayout, UnitGroupRecord},
        media::{
            CodingId, CodingRecord, DataIntegrity, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN,
            MEDIA_VERSION, MediaSectionKind, output::PayloadOutput,
        },
        wire::write_u16_le,
    };

    fn payload(references: [ReferenceMode; 2]) -> Vec<u8> {
        payload_with_map(references, &[1, 2, 2])
    }

    fn payload_with_map(references: [ReferenceMode; 2], frame_ends: &[u32]) -> Vec<u8> {
        let sequence = FrameSequence::new(3, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(2)
            .unwrap();
        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let records = [
            UnitGroupRecord::new(0, 0..1)
                .unwrap()
                .with_reference(references[0]),
            UnitGroupRecord::new(0, 1..2)
                .unwrap()
                .with_reference(references[1]),
        ];
        let map = FrameMapAsset::new(frame_ends).unwrap();
        let durations = FrameTimingAsset::new(&[40, 80, 40], 40).unwrap();
        let compositions = [FrameCompositionOverride::new(
            1,
            FrameComposition::new(BlendMode::SourceOver, DisposalMode::Keep),
        )];
        let compositions = FrameCompositionAsset::new(&compositions, sequence, surface).unwrap();
        let keyframes = [0];
        let keyframes = KeyframeIndexAsset::new(&keyframes, sequence).unwrap();
        let coding = [CodingRecord::new(CodingId::new(42), 1, &[])];
        let coding_len = CodingTable::encoded_len(&coding).unwrap();
        let section_count = 9usize;
        let directory_end = MEDIA_HEADER_LEN + section_count * MEDIA_SECTION_LEN;
        let sequence_offset = directory_end;
        let surface_offset = sequence_offset + FRAME_SEQUENCE_RECORD_LEN;
        let coding_offset = surface_offset + SURFACE_RECORD_LEN;
        let groups_offset = coding_offset + coding_len;
        let map_offset = groups_offset + records.len() * UNIT_GROUP_RECORD_LEN;
        let timing_offset = map_offset + map.encoded_len();
        let composition_offset = timing_offset + durations.encoded_len();
        let keyframes_offset = composition_offset + compositions.encoded_len();
        let data_offset = keyframes_offset + keyframes.encoded_len();
        let payload_len = data_offset + 2 + 4;
        let mut bytes = vec![0; payload_len];
        let mut output = PayloadOutput::buffer(&mut bytes);
        let mut header = [0; MEDIA_HEADER_LEN];
        header[0] = MEDIA_VERSION;
        write_u16_le(&mut header, 2, section_count as u16);
        output.header(&header);
        for (kind, offset, size) in [
            (
                MediaSectionKind::SEQUENCE,
                sequence_offset,
                FRAME_SEQUENCE_RECORD_LEN,
            ),
            (
                MediaSectionKind::SURFACE,
                surface_offset,
                SURFACE_RECORD_LEN,
            ),
            (MediaSectionKind::CODINGS, coding_offset, coding_len),
            (
                MediaSectionKind::UNIT_GROUPS,
                groups_offset,
                records.len() * UNIT_GROUP_RECORD_LEN,
            ),
            (MediaSectionKind::FRAME_MAP, map_offset, map.encoded_len()),
            (
                MediaSectionKind::FRAME_TIMING,
                timing_offset,
                durations.encoded_len(),
            ),
            (
                MediaSectionKind::FRAME_COMPOSITION,
                composition_offset,
                compositions.encoded_len(),
            ),
            (
                MediaSectionKind::KEYFRAME_INDEX,
                keyframes_offset,
                keyframes.encoded_len(),
            ),
            (MediaSectionKind::DATA, data_offset, 2),
        ] {
            output.section(kind, offset, size);
        }
        output.write(&sequence.encode_record());
        let mut surface_bytes = [0; SURFACE_RECORD_LEN];
        surface.encode_record_into(&mut surface_bytes).unwrap();
        output.write(&surface_bytes);
        let mut coding_bytes = vec![0; coding_len];
        CodingTable::encode_into(&coding, &mut coding_bytes).unwrap();
        output.write(&coding_bytes);
        for record in records {
            let mut group = [0; UNIT_GROUP_RECORD_LEN];
            record.encode_into(&mut group).unwrap();
            output.write(&group);
        }
        let mut map_bytes = vec![0; map.encoded_len()];
        map.encode_into(&mut map_bytes).unwrap();
        output.write(&map_bytes);
        let mut timing_bytes = vec![0; durations.encoded_len()];
        durations.encode_into(&mut timing_bytes).unwrap();
        output.write(&timing_bytes);
        let mut composition_bytes = vec![0; compositions.encoded_len()];
        compositions.encode_into(&mut composition_bytes).unwrap();
        output.write(&composition_bytes);
        let mut keyframe_bytes = vec![0; keyframes.encoded_len()];
        keyframes.encode_into(&mut keyframe_bytes).unwrap();
        output.write(&keyframe_bytes);
        output.begin_data(DataIntegrity::Whole);
        output.write(&[0xaa, 0xbb]);
        assert!(output.finish());
        bytes
    }

    #[test]
    fn sectioned_metadata_resolves_defaults_overrides_references_and_noop_frames() {
        let bytes = payload([ReferenceMode::Independent, ReferenceMode::Previous]);
        let frames = SectionedFramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
        assert_eq!(frames.sequence().frame_count(), 3);
        assert_eq!(frames.surface().width(), 2);
        assert_eq!(frames.group_count(), 2);
        assert_eq!(frames.frame(0).unwrap().groups(), 0..1);
        assert_eq!(frames.frame(0).unwrap().duration_ticks(), 40);
        assert_eq!(frames.frame(1).unwrap().duration_ticks(), 80);
        assert_eq!(
            frames.frame(1).unwrap().composition().blend(),
            BlendMode::SourceOver
        );
        assert!(frames.frame(2).unwrap().is_noop());
        assert_eq!(frames.keyframes().unwrap().previous(2), Some(0));
        frames.validate_data().unwrap();
    }

    #[test]
    fn first_frame_and_per_frame_reference_modes_are_strict() {
        let first = payload([ReferenceMode::Previous, ReferenceMode::Previous]);
        assert_eq!(
            SectionedFramesView::open(&first, &PayloadLimits::HOST),
            Err(SectionedFramesError::FirstFrameDependsOnPrevious)
        );
        let mixed = payload([ReferenceMode::Independent, ReferenceMode::Independent]);
        let frames = SectionedFramesView::open(&mixed, &PayloadLimits::HOST).unwrap();
        assert_eq!(frames.frame(1).unwrap().groups(), 1..2);
    }

    #[test]
    fn frame_and_group_limits_apply_before_runtime_materialization() {
        let bytes = payload([ReferenceMode::Independent, ReferenceMode::Previous]);
        assert_eq!(
            SectionedFramesView::open(&bytes, &PayloadLimits::HOST.with_max_frame_records(2),),
            Err(SectionedFramesError::TooManyFrames { count: 3, limit: 2 })
        );
        assert_eq!(
            SectionedFramesView::open(&bytes, &PayloadLimits::HOST.with_max_raster_groups(1),),
            Err(SectionedFramesError::TooManyGroups { count: 2, limit: 1 })
        );
        assert_eq!(
            SectionedFramesView::open(&bytes, &PayloadLimits::HOST.with_max_raster_work(0),),
            Err(SectionedFramesError::Storage(EncodedImageError::Coverage(
                CoverageError::BudgetExceeded
            )))
        );
    }

    #[test]
    fn every_independent_frame_must_cover_the_surface_exactly() {
        let bytes = payload_with_map(
            [ReferenceMode::Independent, ReferenceMode::Independent],
            &[2, 2, 2],
        );
        assert_eq!(
            SectionedFramesView::open(&bytes, &PayloadLimits::HOST),
            Err(SectionedFramesError::Storage(EncodedImageError::Coverage(
                CoverageError::AreaMismatch {
                    plane: 0,
                    expected: 2,
                    actual: 4,
                }
            )))
        );
    }
}
