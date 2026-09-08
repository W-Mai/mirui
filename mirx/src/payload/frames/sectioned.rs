use alloc::vec::Vec;
use core::ops::Range;

use super::{
    FRAME_SEQUENCE_RECORD_LEN, FrameComposition, FrameCompositionAsset, FrameCompositionError,
    FrameCompositionOverride, FrameCompositionTable, FrameCounts, FrameMap, FrameMapError,
    FrameSequence, FrameSequenceError, FrameTiming, FrameTimingAsset, FrameTimingEncoding,
    FrameTimingError, KeyframeIndex, KeyframeIndexAsset, KeyframeIndexError,
};

/// Failure while validating or encoding a FRAMES asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FramesEncodeError {
    Storage(crate::image::ImageEncodeError),
    FrameMap(FrameMapError),
    FrameCountMismatch {
        expected: u32,
        actual: usize,
    },
    GroupCountMismatch {
        expected: u32,
        actual: usize,
    },
    Timing(FrameTimingError),
    Composition(FrameCompositionError),
    Keyframes(KeyframeIndexError),
    ReferenceStorage(EncodedImageError),
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
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    SizeOverflow,
    AllocationFailed,
}
use crate::{
    PayloadLimits,
    image::{
        CodingRecords, ColorTableError, CoverageBudget, EncodedImageAsset, EncodedImageError,
        EncodedStoragePlan, GroupRecords, GroupSource, ReferenceMode, SURFACE_RECORD_LEN,
        SurfaceDescriptor, SurfaceRecordError, UNIT_GROUP_RECORD_LEN, UnitGroupRecord,
    },
    media::{
        CodingRecord, CodingTable, CodingTableError, DataIntegrity, IntegrityRange,
        MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION, MediaFlags, MediaPayload,
        MediaPayloadError, MediaSection, MediaSectionKind, UnitIndex, output::PayloadOutput,
    },
    palette::ColorTableView,
    wire::write_u16_le,
};

/// Sectioned FRAMES metadata without decoded samples or a materialized group table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramesView<'a> {
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

/// Prepared groups for one frame, borrowing caller-owned slots and source bytes.
#[derive(Clone, Debug)]
pub struct FrameGroups<'a, 'g> {
    frames: FramesView<'a>,
    presentation: FramePresentation,
    group_start: usize,
    groups: &'g [Option<crate::image::UnitGroup<'a>>],
}

/// Exact-size iterator over prepared groups for one frame.
#[derive(Clone, Debug)]
pub struct FrameGroupIter<'a, 'g> {
    groups: core::slice::Iter<'g, Option<crate::image::UnitGroup<'a>>>,
}

/// Borrowed canonical FRAMES authoring input.
#[derive(Clone, Copy, Debug)]
pub struct FramesAsset<'a> {
    sequence: FrameSequence,
    surface: SurfaceDescriptor,
    codings: &'a [CodingRecord<'a>],
    groups: &'a [UnitGroupRecord],
    indexes: &'a [u8],
    map: FrameCounts<'a>,
    timing: Option<FrameTimingAsset<'a>>,
    composition: Option<FrameCompositionAsset<'a>>,
    keyframes: Option<KeyframeIndexAsset<'a>>,
    integrity: DataIntegrity<'a>,
    data: &'a [u8],
    color_table: Option<&'a [u8]>,
}

impl<'a> FramesAsset<'a> {
    /// Defines one sequence from shared coding/group storage and per-frame group counts.
    pub fn new(
        sequence: FrameSequence,
        surface: SurfaceDescriptor,
        codings: &'a [CodingRecord<'a>],
        groups: &'a [UnitGroupRecord],
        frame_group_counts: &'a [u32],
        data: &'a [u8],
    ) -> Result<Self, FramesEncodeError> {
        let map = FrameCounts::new(frame_group_counts).map_err(FramesEncodeError::FrameMap)?;
        if map.frame_count() != sequence.frame_count() as usize {
            return Err(FramesEncodeError::FrameCountMismatch {
                expected: sequence.frame_count(),
                actual: map.frame_count(),
            });
        }
        if map.group_count() as usize != groups.len() {
            return Err(FramesEncodeError::GroupCountMismatch {
                expected: map.group_count(),
                actual: groups.len(),
            });
        }
        Ok(Self {
            sequence,
            surface,
            codings,
            groups,
            indexes: &[],
            map,
            timing: None,
            composition: None,
            keyframes: None,
            integrity: DataIntegrity::Whole,
            data,
            color_table: None,
        })
    }

    pub fn with_durations(mut self, durations: &'a [u32]) -> Result<Self, FramesEncodeError> {
        if durations.len() != self.sequence.frame_count() as usize {
            return Err(FramesEncodeError::FrameCountMismatch {
                expected: self.sequence.frame_count(),
                actual: durations.len(),
            });
        }
        self.timing = Some(
            FrameTimingAsset::new(durations, self.sequence.default_duration_ticks())
                .map_err(FramesEncodeError::Timing)?,
        );
        Ok(self)
    }

    pub fn with_composition(
        mut self,
        overrides: &'a [FrameCompositionOverride],
    ) -> Result<Self, FramesEncodeError> {
        self.composition = if overrides.is_empty() {
            None
        } else {
            Some(
                FrameCompositionAsset::new(overrides, self.sequence, self.surface)
                    .map_err(FramesEncodeError::Composition)?,
            )
        };
        Ok(self)
    }

    pub fn with_keyframes(mut self, frames: &'a [u32]) -> Result<Self, FramesEncodeError> {
        self.keyframes = Some(
            KeyframeIndexAsset::new(frames, self.sequence).map_err(FramesEncodeError::Keyframes)?,
        );
        Ok(self)
    }

    /// Supplies encoded unit-selection and byte-range indexes referenced by group records.
    pub const fn with_unit_index(mut self, bytes: &'a [u8]) -> Self {
        self.indexes = bytes;
        self
    }

    pub const fn with_integrity(mut self, integrity: DataIntegrity<'a>) -> Self {
        self.integrity = integrity;
        self
    }

    pub const fn with_color_table(mut self, rgba: &'a [u8]) -> Self {
        self.color_table = Some(rgba);
        self
    }

    pub const fn sequence(self) -> FrameSequence {
        self.sequence
    }

    pub const fn surface(self) -> SurfaceDescriptor {
        self.surface
    }

    pub const fn frame_map(self) -> FrameCounts<'a> {
        self.map
    }

    pub fn encoded_len(self) -> Result<usize, FramesEncodeError> {
        Ok(FramesPlan::new(self)?.payload_len)
    }

    /// Writes one canonical payload after completing validation and sizing.
    pub fn encode_into(self, output: &mut [u8]) -> Result<usize, FramesEncodeError> {
        let plan = FramesPlan::new(self)?;
        if output.len() < plan.payload_len {
            return Err(FramesEncodeError::BufferTooSmall {
                needed: plan.payload_len,
                available: output.len(),
            });
        }
        let len = plan.payload_len;
        plan.emit(PayloadOutput::buffer(&mut output[..len]));
        Ok(len)
    }

    /// Allocates exactly the canonical payload length.
    pub fn encode(self) -> Result<Vec<u8>, FramesEncodeError> {
        let plan = FramesPlan::new(self)?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(plan.payload_len)
            .map_err(|_| FramesEncodeError::AllocationFailed)?;
        output.resize(plan.payload_len, 0);
        plan.emit(PayloadOutput::buffer(&mut output));
        Ok(output)
    }

    fn storage(self) -> EncodedImageAsset<'a> {
        let mut storage =
            EncodedImageAsset::from_groups(self.surface, self.codings, self.groups, self.data)
                .with_unit_index(self.indexes)
                .with_integrity(self.integrity);
        if let Some(table) = self.color_table {
            storage = storage.with_color_table(table);
        }
        storage
    }
}

struct FramesPlan<'a> {
    asset: FramesAsset<'a>,
    storage: EncodedStoragePlan<'a>,
    integrity_len: usize,
    section_count: u16,
    data_offset: usize,
    payload_len: usize,
}

impl<'a> FramesPlan<'a> {
    fn new(asset: FramesAsset<'a>) -> Result<Self, FramesEncodeError> {
        let storage =
            EncodedStoragePlan::new(asset.storage()).map_err(FramesEncodeError::Storage)?;
        let source = storage.source();
        source
            .visit_groups_with_references(None, |_, _| {})
            .map_err(FramesEncodeError::ReferenceStorage)?;
        validate_asset_references(asset, source)?;

        let timing_len = asset
            .timing
            .filter(|timing| !timing.is_empty())
            .map_or(0, FrameTimingAsset::encoded_len);
        let composition_len = asset
            .composition
            .map_or(0, FrameCompositionAsset::encoded_len);
        let keyframe_len = asset.keyframes.map_or(0, KeyframeIndexAsset::encoded_len);
        let integrity_len = asset
            .integrity
            .section_len(asset.data.len())
            .map_err(crate::image::ImageEncodeError::Integrity)
            .map_err(FramesEncodeError::Storage)?;
        let storage_sections = storage.sections();
        let storage_len = storage_sections
            .iter()
            .flatten()
            .try_fold(0usize, |len, (_, size)| len.checked_add(*size))
            .ok_or(FramesEncodeError::SizeOverflow)?;
        let storage_section_count = storage_sections.iter().flatten().count();
        let section_count = 3usize
            .checked_add(storage_section_count)
            .and_then(|count| count.checked_add(1))
            .and_then(|count| count.checked_add(usize::from(timing_len != 0)))
            .and_then(|count| count.checked_add(usize::from(composition_len != 0)))
            .and_then(|count| count.checked_add(usize::from(keyframe_len != 0)))
            .and_then(|count| count.checked_add(usize::from(asset.color_table.is_some())))
            .and_then(|count| {
                count.checked_add(usize::from(asset.integrity.partitions().is_some()))
            })
            .ok_or(FramesEncodeError::SizeOverflow)?;
        let section_count =
            u16::try_from(section_count).map_err(|_| FramesEncodeError::SizeOverflow)?;
        let metadata_len = MEDIA_HEADER_LEN
            .checked_add(usize::from(section_count) * MEDIA_SECTION_LEN)
            .and_then(|len| len.checked_add(FRAME_SEQUENCE_RECORD_LEN))
            .and_then(|len| len.checked_add(SURFACE_RECORD_LEN))
            .and_then(|len| len.checked_add(storage_len))
            .and_then(|len| len.checked_add(asset.map.encoded_len()))
            .and_then(|len| len.checked_add(timing_len))
            .and_then(|len| len.checked_add(composition_len))
            .and_then(|len| len.checked_add(keyframe_len))
            .and_then(|len| len.checked_add(asset.color_table.map_or(0, <[u8]>::len)))
            .and_then(|len| len.checked_add(integrity_len))
            .ok_or(FramesEncodeError::SizeOverflow)?;
        let data_offset = UnitIndex::aligned(
            u32::try_from(metadata_len).map_err(|_| FramesEncodeError::SizeOverflow)?,
            storage.alignment(),
        )
        .map_err(|_| FramesEncodeError::SizeOverflow)? as usize;
        let payload_len = data_offset
            .checked_add(asset.data.len())
            .and_then(|len| len.checked_add(asset.integrity.trailer_len()))
            .ok_or(FramesEncodeError::SizeOverflow)?;
        u32::try_from(payload_len).map_err(|_| FramesEncodeError::SizeOverflow)?;
        Ok(Self {
            asset,
            storage,
            integrity_len,
            section_count,
            data_offset,
            payload_len,
        })
    }

    fn emit(self, mut output: PayloadOutput<'_>) -> bool {
        let timing = self.asset.timing.filter(|timing| !timing.is_empty());
        let mut header = [0; MEDIA_HEADER_LEN];
        header[0] = MEDIA_VERSION;
        if self.asset.integrity.partitions().is_some() {
            header[1] = MediaFlags::INDEXED_INTEGRITY.bits();
        }
        write_u16_le(&mut header, 2, self.section_count);
        output.header(&header);

        let mut offset = MEDIA_HEADER_LEN + usize::from(self.section_count) * MEDIA_SECTION_LEN;
        output.section(
            MediaSectionKind::SEQUENCE,
            offset,
            FRAME_SEQUENCE_RECORD_LEN,
        );
        offset += FRAME_SEQUENCE_RECORD_LEN;
        output.section(MediaSectionKind::SURFACE, offset, SURFACE_RECORD_LEN);
        offset += SURFACE_RECORD_LEN;
        for (kind, size) in self.storage.sections().into_iter().flatten() {
            output.section(kind, offset, size);
            offset += size;
        }
        output.section(
            MediaSectionKind::FRAME_MAP,
            offset,
            self.asset.map.encoded_len(),
        );
        offset += self.asset.map.encoded_len();
        if let Some(timing) = timing {
            output.section(MediaSectionKind::FRAME_TIMING, offset, timing.encoded_len());
            offset += timing.encoded_len();
        }
        if let Some(composition) = self.asset.composition {
            output.section(
                MediaSectionKind::FRAME_COMPOSITION,
                offset,
                composition.encoded_len(),
            );
            offset += composition.encoded_len();
        }
        if let Some(keyframes) = self.asset.keyframes {
            output.section(
                MediaSectionKind::KEYFRAME_INDEX,
                offset,
                keyframes.encoded_len(),
            );
            offset += keyframes.encoded_len();
        }
        if let Some(table) = self.asset.color_table {
            output.section(MediaSectionKind::COLOR_TABLE, offset, table.len());
            offset += table.len();
        }
        if self.asset.integrity.partitions().is_some() {
            output.section(MediaSectionKind::INTEGRITY, offset, self.integrity_len);
        }
        output.section(
            MediaSectionKind::DATA,
            self.data_offset,
            self.asset.data.len(),
        );

        output.write(&self.asset.sequence.encode_record());
        let mut surface = [0; SURFACE_RECORD_LEN];
        self.asset
            .surface
            .encode_record_into(&mut surface)
            .expect("validated surface record");
        output.write(&surface);
        self.storage.emit_metadata(&mut output);
        write_frame_map(self.asset.map, &mut output);
        if let Some(timing) = timing {
            write_frame_timing(timing, &mut output);
        }
        if let Some(composition) = self.asset.composition {
            for record in composition.records() {
                output.write(&record.encode_record());
            }
        }
        if let Some(keyframes) = self.asset.keyframes {
            for &frame in keyframes.frames() {
                output.write(&frame.to_le_bytes()[..keyframes.entry_bytes()]);
            }
        }
        if let Some(table) = self.asset.color_table {
            output.write(table);
        }
        write_integrity(
            &mut output,
            self.asset.integrity,
            self.data_offset,
            self.asset.data,
        );
        output.pad_to(self.data_offset);
        output.begin_data(self.asset.integrity);
        output.write(self.asset.data);
        debug_assert_eq!(
            output.position() + self.asset.integrity.trailer_len(),
            self.payload_len
        );
        output.finish()
    }
}

fn validate_asset_references(
    asset: FramesAsset<'_>,
    source: GroupSource<'_>,
) -> Result<(), FramesEncodeError> {
    let mut budget = CoverageBudget::new(u64::MAX);
    let mut start = 0u32;
    let mut delta_frames = 0u32;
    let indexed = asset.keyframes.map_or(&[][..], KeyframeIndexAsset::frames);
    let mut indexed_position = 0usize;
    for (frame, &count) in asset.map.counts().iter().enumerate() {
        let frame = frame as u32;
        let end = start + count;
        let range = start..end;
        let reference =
            frame_reference(source, frame, range.clone()).map_err(map_reference_storage)?;
        if indexed.get(indexed_position) == Some(&frame) {
            if reference != ReferenceMode::Independent {
                return Err(FramesEncodeError::IndexedFrameDependsOnPrevious { frame });
            }
            indexed_position += 1;
        }
        match reference {
            ReferenceMode::Independent => {
                source
                    .validate_group_range(range.start as usize..range.end as usize, &mut budget)
                    .map_err(FramesEncodeError::ReferenceStorage)?;
                delta_frames = 0;
            }
            ReferenceMode::Previous => {
                if frame == 0 {
                    return Err(FramesEncodeError::FirstFrameDependsOnPrevious);
                }
                source
                    .validate_disjoint_group_range(
                        range.start as usize..range.end as usize,
                        &mut budget,
                    )
                    .map_err(FramesEncodeError::ReferenceStorage)?;
                delta_frames += 1;
                if delta_frames > u32::from(asset.sequence.max_delta_frames()) {
                    return Err(FramesEncodeError::DeltaBoundExceeded {
                        frame,
                        delta_frames,
                        limit: asset.sequence.max_delta_frames(),
                    });
                }
            }
        }
        start = end;
    }
    debug_assert_eq!(indexed_position, indexed.len());
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrameReferenceError {
    Storage(EncodedImageError),
    Mixed { frame: u32 },
}

fn frame_reference(
    source: GroupSource<'_>,
    frame: u32,
    range: Range<u32>,
) -> Result<ReferenceMode, FrameReferenceError> {
    if range.is_empty() {
        return Ok(ReferenceMode::Previous);
    }
    let first = source
        .record(range.start as usize)
        .map_err(FrameReferenceError::Storage)?
        .reference();
    for group in range.start + 1..range.end {
        let next = source
            .record(group as usize)
            .map_err(FrameReferenceError::Storage)?
            .reference();
        if next != first {
            return Err(FrameReferenceError::Mixed { frame });
        }
    }
    Ok(first)
}

fn map_reference_storage(error: FrameReferenceError) -> FramesEncodeError {
    match error {
        FrameReferenceError::Storage(error) => FramesEncodeError::ReferenceStorage(error),
        FrameReferenceError::Mixed { frame } => FramesEncodeError::MixedReferences { frame },
    }
}

fn write_frame_map(map: FrameCounts<'_>, output: &mut PayloadOutput<'_>) {
    let entry_bytes = map.encoded_len() / map.frame_count();
    let mut end = 0u32;
    for &count in map.counts() {
        end += count;
        output.write(&end.to_le_bytes()[..entry_bytes]);
    }
}

fn write_frame_timing(timing: FrameTimingAsset<'_>, output: &mut PayloadOutput<'_>) {
    let encoding = timing.encoding().expect("nonempty timing section");
    output.write(&[
        match encoding {
            FrameTimingEncoding::Dense => 0,
            FrameTimingEncoding::Sparse => 1,
        },
        0,
        0,
        0,
    ]);
    for (frame, &duration) in timing.durations().iter().enumerate() {
        if encoding == FrameTimingEncoding::Sparse && duration == timing.default_duration_ticks() {
            continue;
        }
        if encoding == FrameTimingEncoding::Sparse {
            output.write(&(frame as u32).to_le_bytes());
        }
        output.write(&duration.to_le_bytes());
    }
}

fn write_integrity(
    output: &mut PayloadOutput<'_>,
    integrity: DataIntegrity<'_>,
    data_offset: usize,
    data: &[u8],
) {
    let mut start = 0u32;
    for &end in integrity.partitions().unwrap_or(&[]) {
        let checksum = crate::crc32(&data[start as usize..end as usize]);
        let range = IntegrityRange::new(
            data_offset as u32 + start..data_offset as u32 + end,
            checksum,
        )
        .expect("validated integrity partition");
        output.write(&range.encode_record());
        start = end;
    }
}

impl<'a> FramesView<'a> {
    /// Opens metadata without decoding or scanning DATA bytes.
    pub fn open(payload: &'a [u8], limits: &PayloadLimits) -> Result<Self, FramesError> {
        let media = MediaPayload::open(payload).map_err(FramesError::Media)?;
        Self::from_media(media, None, limits)
    }

    /// Retains the outer payload position for file-relative alignment checks.
    pub fn open_at(
        payload: &'a [u8],
        file_offset: u32,
        limits: &PayloadLimits,
    ) -> Result<Self, FramesError> {
        let media = MediaPayload::open(payload).map_err(FramesError::Media)?;
        Self::from_media(media, Some(file_offset), limits)
    }

    fn from_media(
        media: MediaPayload<'a>,
        file_offset: Option<u32>,
        limits: &PayloadLimits,
    ) -> Result<Self, FramesError> {
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
                    return Err(FramesError::UnexpectedSection(kind));
                }
                _ if section.descriptor().flags().is_required() => {
                    return Err(FramesError::UnknownRequiredSection(kind));
                }
                _ => None,
            };
            if let Some(slot) = slot {
                if slot.replace(section).is_some() {
                    return Err(FramesError::DuplicateSection(kind));
                }
                if !section.descriptor().flags().is_required() {
                    return Err(FramesError::SectionMustBeRequired(kind));
                }
            }
        }
        let sequence = required(sequence, MediaSectionKind::SEQUENCE)?;
        require_size(sequence, FRAME_SEQUENCE_RECORD_LEN)?;
        let sequence = FrameSequence::open(sequence.bytes()).map_err(FramesError::Sequence)?;
        if sequence.frame_count() > limits.max_frame_records() {
            return Err(FramesError::TooManyFrames {
                count: sequence.frame_count(),
                limit: limits.max_frame_records(),
            });
        }

        let surface_section = required(surface, MediaSectionKind::SURFACE)?;
        require_size(surface_section, SURFACE_RECORD_LEN)?;
        let surface = SurfaceDescriptor::from_record(surface_section.bytes())
            .map_err(FramesError::Surface)?;
        let codings = CodingTable::open(required(codings, MediaSectionKind::CODINGS)?.bytes())
            .map_err(FramesError::Codings)?;
        let groups = required(groups, MediaSectionKind::UNIT_GROUPS)?;
        if groups.bytes().is_empty() || groups.bytes().len() % UNIT_GROUP_RECORD_LEN != 0 {
            return Err(FramesError::InvalidGroupTableLength(groups.bytes().len()));
        }
        let group_count = groups.bytes().len() / UNIT_GROUP_RECORD_LEN;
        if group_count as u64 > u64::from(limits.max_raster_groups()) {
            return Err(FramesError::TooManyGroups {
                count: group_count,
                limit: limits.max_raster_groups(),
            });
        }
        let map = FrameMap::open(
            required(map, MediaSectionKind::FRAME_MAP)?.bytes(),
            sequence.frame_count(),
        )
        .map_err(FramesError::Map)?;
        if map.group_count() as usize != group_count {
            return Err(FramesError::GroupCountMismatch {
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
            .map_err(FramesError::Timing)?;
        let composition = composition
            .map(|section| FrameCompositionTable::open(section.bytes(), sequence, surface))
            .transpose()
            .map_err(FramesError::Composition)?;
        let keyframes = keyframes
            .map(|section| KeyframeIndex::open(section.bytes(), sequence))
            .transpose()
            .map_err(FramesError::Keyframes)?;
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
            .map_err(FramesError::Storage)?;
        view.validate_reference_runs(&mut budget)?;
        Ok(view)
    }

    pub(crate) const fn media(self) -> MediaPayload<'a> {
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

    /// Finds the closest independent frame that can reconstruct `frame`.
    ///
    /// An explicit keyframe index provides the answer directly. Payloads that
    /// omit the index are scanned only within the declared delta bound.
    pub fn recovery_frame(self, frame: u32) -> Option<u32> {
        if frame >= self.sequence.frame_count() {
            return None;
        }
        if let Some(keyframes) = self.keyframes {
            return keyframes.previous(frame);
        }
        let source = self.group_source();
        let earliest = frame.saturating_sub(u32::from(self.sequence.max_delta_frames()));
        for candidate in (earliest..=frame).rev() {
            let range = self.map.get(candidate)?;
            if range.is_empty() {
                continue;
            }
            if source.record(range.start as usize).ok()?.reference() == ReferenceMode::Independent {
                return Some(candidate);
            }
        }
        None
    }

    pub const fn color_table(self) -> Option<ColorTableView<'a>> {
        self.color_table
    }

    pub const fn group_count(self) -> usize {
        self.groups.bytes().len() / UNIT_GROUP_RECORD_LEN
    }

    /// Maximum declared source alignment across stored frame groups.
    ///
    /// The value constrains the FRAMES payload's file or Flash placement; it
    /// does not describe the alignment required by a decoded destination.
    pub fn input_alignment(self) -> Result<crate::ByteAlignment, FramesError> {
        self.group_source()
            .input_alignment()
            .map_err(FramesError::Storage)
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

    /// Resolves one frame's groups into caller-owned slots without allocation.
    ///
    /// A capacity error preserves every slot. Other failures may overwrite the
    /// used prefix; only a returned handle guarantees prepared groups.
    pub fn groups_into<'g>(
        self,
        frame: u32,
        workspace: &'g mut [Option<crate::image::UnitGroup<'a>>],
        budget: &mut CoverageBudget,
    ) -> Result<FrameGroups<'a, 'g>, FramesError> {
        let presentation = self
            .frame(frame)
            .ok_or(FramesError::FrameOutOfBounds(frame))?;
        let range = presentation.groups();
        let count = range.len();
        if workspace.len() < count {
            return Err(FramesError::WorkspaceTooSmall {
                needed: count,
                available: workspace.len(),
            });
        }
        let workspace = &mut workspace[..count];
        workspace.fill(None);
        let source = self.group_source();
        for (relative, global) in (range.start as usize..range.end as usize).enumerate() {
            let record = source.record(global).map_err(FramesError::Storage)?;
            budget
                .spend_many(source.resolution_cost(record))
                .map_err(|error| FramesError::Storage(EncodedImageError::Coverage(error)))?;
            workspace[relative] = Some(
                source
                    .resolve_record(global, record)
                    .map_err(FramesError::Storage)?
                    .0,
            );
        }
        Ok(FrameGroups {
            frames: self,
            presentation,
            group_start: range.start as usize,
            groups: workspace,
        })
    }

    pub fn validate_data(self) -> Result<(), FramesError> {
        self.media.validate_data().map_err(FramesError::Media)
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

    fn validate_reference_runs(self, budget: &mut CoverageBudget) -> Result<(), FramesError> {
        let source = self.group_source();
        let mut delta_frames = 0u32;
        for frame in 0..self.sequence.frame_count() {
            let range = self.map.get(frame).expect("validated frame map");
            let reference = if range.is_empty() {
                ReferenceMode::Previous
            } else {
                let first = source
                    .record(range.start as usize)
                    .map_err(FramesError::Storage)?
                    .reference();
                for group in range.start + 1..range.end {
                    let next = source
                        .record(group as usize)
                        .map_err(FramesError::Storage)?
                        .reference();
                    if next != first {
                        return Err(FramesError::MixedReferences { frame });
                    }
                }
                first
            };
            match reference {
                ReferenceMode::Independent => {
                    source
                        .validate_group_range(range.start as usize..range.end as usize, budget)
                        .map_err(FramesError::Storage)?;
                    delta_frames = 0;
                }
                ReferenceMode::Previous => {
                    if frame == 0 {
                        return Err(FramesError::FirstFrameDependsOnPrevious);
                    }
                    source
                        .validate_disjoint_group_range(
                            range.start as usize..range.end as usize,
                            budget,
                        )
                        .map_err(FramesError::Storage)?;
                    delta_frames += 1;
                    if delta_frames > u32::from(self.sequence.max_delta_frames()) {
                        return Err(FramesError::DeltaBoundExceeded {
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
                        .map_err(FramesError::Storage)?
                        .reference()
                        != ReferenceMode::Independent
                {
                    return Err(FramesError::IndexedFrameDependsOnPrevious { frame });
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

impl<'a, 'g> FrameGroups<'a, 'g> {
    pub const fn frames(&self) -> FramesView<'a> {
        self.frames
    }

    pub fn presentation(&self) -> FramePresentation {
        self.presentation.clone()
    }

    pub const fn len(&self) -> usize {
        self.groups.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<crate::image::UnitGroup<'a>> {
        self.groups.get(index).copied().flatten()
    }

    pub fn iter(&self) -> FrameGroupIter<'a, 'g> {
        FrameGroupIter {
            groups: self.groups.iter(),
        }
    }

    pub fn reference(&self) -> ReferenceMode {
        self.get(0)
            .map_or(ReferenceMode::Previous, |group| group.reference())
    }

    /// Verifies checksum coverage for one encoded unit before it is decoded.
    pub fn validate_unit(&self, group: usize, ordinal: usize) -> Result<u32, FramesError> {
        let plan = self.unit_check_plan(group, ordinal)?;
        let byte_len = plan.byte_len();
        plan.verify().map_err(FramesError::Media)?;
        Ok(byte_len)
    }

    /// Plans exact checksum work for one encoded unit without reading DATA.
    pub fn unit_check_plan(
        &self,
        group: usize,
        ordinal: usize,
    ) -> Result<crate::media::DataCheckPlan<'a>, FramesError> {
        let unit = self
            .get(group)
            .ok_or(FramesError::GroupOutOfBounds(group))?
            .get(ordinal)
            .ok_or(FramesError::UnitOutOfBounds { group, ordinal })?;
        let source = self.frames.group_source();
        let base = self
            .frames
            .data
            .descriptor()
            .offset()
            .checked_add(
                source
                    .record(self.group_start + group)
                    .map_err(FramesError::Storage)?
                    .data_range()
                    .start,
            )
            .ok_or(FramesError::SizeOverflow)?;
        let range = unit.data_range();
        let start = base
            .checked_add(range.start)
            .ok_or(FramesError::SizeOverflow)?;
        let end = base
            .checked_add(range.end)
            .ok_or(FramesError::SizeOverflow)?;
        self.frames
            .media
            .data_check_plan(start..end)
            .map_err(FramesError::Media)
    }
}

impl<'a> Iterator for FrameGroupIter<'a, '_> {
    type Item = crate::image::UnitGroup<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.groups
            .next()
            .map(|group| group.expect("prepared frame group"))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.groups
            .nth(n)
            .map(|group| group.expect("prepared frame group"))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.groups.size_hint()
    }
}

impl DoubleEndedIterator for FrameGroupIter<'_, '_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.groups
            .next_back()
            .map(|group| group.expect("prepared frame group"))
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.groups
            .nth_back(n)
            .map(|group| group.expect("prepared frame group"))
    }
}

impl ExactSizeIterator for FrameGroupIter<'_, '_> {}
impl core::iter::FusedIterator for FrameGroupIter<'_, '_> {}

fn required<'a>(
    section: Option<MediaSection<'a>>,
    kind: MediaSectionKind,
) -> Result<MediaSection<'a>, FramesError> {
    section.ok_or(FramesError::MissingSection(kind))
}

fn require_size(section: MediaSection<'_>, expected: usize) -> Result<(), FramesError> {
    if section.bytes().len() != expected {
        return Err(FramesError::SectionSizeMismatch {
            kind: section.descriptor().kind(),
            expected,
            actual: section.bytes().len(),
        });
    }
    Ok(())
}

fn map_color_table(error: ColorTableError) -> FramesError {
    match error {
        ColorTableError::Missing => FramesError::MissingColorTable,
        ColorTableError::Unexpected => FramesError::UnexpectedColorTable,
        ColorTableError::SizeMismatch { expected, actual } => FramesError::SectionSizeMismatch {
            kind: MediaSectionKind::COLOR_TABLE,
            expected,
            actual,
        },
        ColorTableError::SizeOverflow => FramesError::SizeOverflow,
    }
}

/// Failure while opening sectioned FRAMES metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FramesError {
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
    FrameOutOfBounds(u32),
    WorkspaceTooSmall {
        needed: usize,
        available: usize,
    },
    GroupOutOfBounds(usize),
    UnitOutOfBounds {
        group: usize,
        ordinal: usize,
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
        coding::CodingId,
        frames::{
            BlendMode, DisposalMode, FrameCompositionAsset, FrameCompositionOverride,
            FrameMapAsset, FrameTimingAsset, KeyframeIndexAsset,
        },
        image::{ColorDescription, CoverageError, SampleLayout, UnitGroupRecord},
        media::{
            CodingRecord, DataIntegrity, MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MEDIA_VERSION,
            MediaSectionKind, output::PayloadOutput,
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
        let frames = FramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
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
        let mut empty = [];
        assert!(matches!(
            frames.groups_into(1, &mut empty, &mut CoverageBudget::new(100)),
            Err(FramesError::WorkspaceTooSmall {
                needed: 1,
                available: 0,
            })
        ));
        let mut slots = [None];
        let groups = frames
            .groups_into(1, &mut slots, &mut CoverageBudget::new(100))
            .unwrap();
        assert_eq!(groups.presentation().index(), 1);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups.iter().len(), 1);
        assert_eq!(groups.get(0).unwrap().reference(), ReferenceMode::Previous);
        assert_eq!(groups.validate_unit(0, 0).unwrap(), 2);
        frames.validate_data().unwrap();
    }

    #[test]
    fn canonical_asset_roundtrips_every_optional_column() {
        let sequence = FrameSequence::new(3, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(2)
            .unwrap();
        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let codings = [CodingRecord::new(CodingId::new(42), 1, &[])];
        let groups = [
            UnitGroupRecord::new(0, 0..1).unwrap(),
            UnitGroupRecord::new(0, 1..2)
                .unwrap()
                .with_reference(ReferenceMode::Previous),
        ];
        let durations = [40, 80, 40];
        let compositions = [FrameCompositionOverride::new(
            1,
            FrameComposition::new(BlendMode::SourceOver, DisposalMode::Keep),
        )];
        let keyframes = [0];
        let asset = FramesAsset::new(
            sequence,
            surface,
            &codings,
            &groups,
            &[1, 1, 0],
            &[0xaa, 0xbb],
        )
        .unwrap()
        .with_durations(&durations)
        .unwrap()
        .with_composition(&compositions)
        .unwrap()
        .with_keyframes(&keyframes)
        .unwrap();
        let needed = asset.encoded_len().unwrap();
        let mut short = vec![0x5a; needed - 1];
        assert_eq!(
            asset.encode_into(&mut short),
            Err(FramesEncodeError::BufferTooSmall {
                needed,
                available: needed - 1,
            })
        );
        assert!(short.iter().all(|&byte| byte == 0x5a));

        let bytes = asset.encode().unwrap();
        assert_eq!(bytes.len(), needed);
        let frames = FramesView::open(&bytes, &PayloadLimits::HOST).unwrap();
        assert_eq!(frames.sequence(), sequence);
        assert_eq!(frames.surface(), surface);
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
    fn authoring_aligns_data_and_integrity_covers_inter_group_padding() {
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        let surface =
            SurfaceDescriptor::new(1, 1, SampleLayout::A8, ColorDescription::NONE).unwrap();
        let codings = [CodingRecord::new(CodingId::new(42), 1, &[])];
        let groups = [
            UnitGroupRecord::new(0, 0..1)
                .unwrap()
                .with_input_alignment(crate::ByteAlignment::new(64).unwrap()),
            UnitGroupRecord::new(0, 64..65)
                .unwrap()
                .with_reference(ReferenceMode::Previous)
                .with_input_alignment(crate::ByteAlignment::new(64).unwrap()),
        ];
        let mut data = [0x5a; 65];
        data[0] = 1;
        data[64] = 2;
        let integrity_ends = [1, 64, 65];
        let asset = FramesAsset::new(sequence, surface, &codings, &groups, &[1, 1], &data)
            .unwrap()
            .with_integrity(DataIntegrity::Indexed(&integrity_ends));
        let mut bytes = asset.encode().unwrap();
        let media = MediaPayload::open(&bytes).unwrap();
        let data_offset = media
            .section(MediaSectionKind::DATA)
            .unwrap()
            .descriptor()
            .offset();
        assert_eq!(data_offset % 64, 0);
        let frames = FramesView::open_at(&bytes, 64, &PayloadLimits::HOST).unwrap();
        frames.validate_data().unwrap();

        bytes[data_offset as usize + 32] ^= 1;
        let frames = FramesView::open_at(&bytes, 64, &PayloadLimits::HOST).unwrap();
        assert!(frames.validate_data().is_err());
    }

    #[test]
    fn first_frame_and_per_frame_reference_modes_are_strict() {
        let first = payload([ReferenceMode::Previous, ReferenceMode::Previous]);
        assert_eq!(
            FramesView::open(&first, &PayloadLimits::HOST),
            Err(FramesError::FirstFrameDependsOnPrevious)
        );
        let mixed = payload([ReferenceMode::Independent, ReferenceMode::Independent]);
        let frames = FramesView::open(&mixed, &PayloadLimits::HOST).unwrap();
        assert_eq!(frames.frame(1).unwrap().groups(), 1..2);
    }

    #[test]
    fn frame_and_group_limits_apply_before_runtime_materialization() {
        let bytes = payload([ReferenceMode::Independent, ReferenceMode::Previous]);
        assert_eq!(
            FramesView::open(&bytes, &PayloadLimits::HOST.with_max_frame_records(2),),
            Err(FramesError::TooManyFrames { count: 3, limit: 2 })
        );
        assert_eq!(
            FramesView::open(&bytes, &PayloadLimits::HOST.with_max_raster_groups(1),),
            Err(FramesError::TooManyGroups { count: 2, limit: 1 })
        );
        assert_eq!(
            FramesView::open(&bytes, &PayloadLimits::HOST.with_max_raster_work(0),),
            Err(FramesError::Storage(EncodedImageError::Coverage(
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
            FramesView::open(&bytes, &PayloadLimits::HOST),
            Err(FramesError::Storage(EncodedImageError::Coverage(
                CoverageError::AreaMismatch {
                    plane: 0,
                    expected: 2,
                    actual: 4,
                }
            )))
        );
    }
}
