use alloc::vec::Vec;

use super::{
    FrameCandidate, FrameChoice, FramePolicy, FrameSelectionError, FrameSelector, FrameSequence,
    FrameStorage, FrameTimingError, FramesAsset, FramesEncodeError,
};
use crate::{
    ByteAlignment,
    coding::{
        CodingId, FrameDelta, FrameDeltaError, Frequency, FrequencyError, FrequencyGeometry, Lz4,
        Lz4Error, Pixel, PixelError, Rle, RleError,
    },
    image::{
        GroupSelection, ReferenceMode, Region, RegionError, SurfaceDescriptor, TileGrid,
        TileGridError, UNIT_GROUP_RECORD_LEN, UnitGroupRecord,
    },
    media::{
        CODING_RECORD_LEN, CodingRecord, UnitIndexEncoding, UnitIndexError, UnitSelectionEncoding,
        UnitSelectionError,
    },
};

/// Lossless coding profiles considered by [`FramesEncoder`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameEncodingSet {
    raw: bool,
    rle: bool,
    pixel: bool,
    lz4: bool,
    frequency_reversible: bool,
    frequency_quality: Option<u8>,
    delta: bool,
}

impl FrameEncodingSet {
    /// Enables every lossless whole-frame profile.
    pub const fn lossless() -> Self {
        Self {
            raw: true,
            rle: true,
            pixel: true,
            lz4: true,
            frequency_reversible: true,
            frequency_quality: None,
            delta: true,
        }
    }

    pub const fn with_raw(mut self, enabled: bool) -> Self {
        self.raw = enabled;
        self
    }

    pub const fn with_rle(mut self, enabled: bool) -> Self {
        self.rle = enabled;
        self
    }

    pub const fn with_pixel(mut self, enabled: bool) -> Self {
        self.pixel = enabled;
        self
    }

    pub const fn with_lz4(mut self, enabled: bool) -> Self {
        self.lz4 = enabled;
        self
    }

    pub const fn with_reversible_frequency(mut self, enabled: bool) -> Self {
        self.frequency_reversible = enabled;
        self
    }

    pub fn with_quantized_frequency(mut self, quality: u8) -> Result<Self, FrequencyError> {
        Frequency::quantized(quality)?;
        self.frequency_quality = Some(quality);
        Ok(self)
    }

    pub const fn without_quantized_frequency(mut self) -> Self {
        self.frequency_quality = None;
        self
    }

    pub const fn with_delta(mut self, enabled: bool) -> Self {
        self.delta = enabled;
        self
    }
}

impl Default for FrameEncodingSet {
    fn default() -> Self {
        Self::lossless()
    }
}

/// Coding selected for one stored frame group.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameEncoding {
    Raw,
    Rle,
    Pixel,
    Lz4,
    FrequencyReversible,
    FrequencyQuantized(u8),
    Delta,
}

impl FrameEncoding {
    pub const fn coding_id(self) -> CodingId {
        match self {
            Self::Raw => CodingId::RAW,
            Self::Rle => CodingId::RLE,
            Self::Pixel => CodingId::PIXEL,
            Self::Lz4 => CodingId::LZ4,
            Self::FrequencyReversible => CodingId::FREQUENCY_REVERSIBLE,
            Self::FrequencyQuantized(_) => CodingId::FREQUENCY_QUANTIZED,
            Self::Delta => CodingId::FRAME_DELTA,
        }
    }

    fn record(&self) -> CodingRecord<'_> {
        match self {
            Self::Raw => CodingRecord::RAW,
            Self::Rle => Rle::new().record(),
            Self::Pixel => CodingRecord::new(CodingId::PIXEL, 1, &[]),
            Self::Lz4 => Lz4::new().record(),
            Self::FrequencyReversible => CodingRecord::new(CodingId::FREQUENCY_REVERSIBLE, 1, &[]),
            Self::FrequencyQuantized(quality) => CodingRecord::new(
                CodingId::FREQUENCY_QUANTIZED,
                1,
                core::slice::from_ref(quality),
            ),
            Self::Delta => FrameDelta::new().record(),
        }
    }

    const fn is_lossless(self) -> bool {
        !matches!(self, Self::FrequencyQuantized(_))
    }
}

/// One frame's selected representation and exact incremental storage cost.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameWriteReport {
    frame: u32,
    storage: FrameStorage,
    encoding: Option<FrameEncoding>,
    encoded_bytes: u32,
    stored_bytes: u64,
    delta_frames: u16,
}

impl FrameWriteReport {
    pub const fn frame(self) -> u32 {
        self.frame
    }

    pub const fn storage(self) -> FrameStorage {
        self.storage
    }

    pub const fn encoding(self) -> Option<FrameEncoding> {
        self.encoding
    }

    pub const fn encoded_bytes(self) -> u32 {
        self.encoded_bytes
    }

    pub const fn stored_bytes(self) -> u64 {
        self.stored_bytes
    }

    pub const fn delta_frames(self) -> u16 {
        self.delta_frames
    }
}

/// Finished canonical FRAMES payload plus deterministic authoring decisions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedFrames {
    payload: Vec<u8>,
    reports: Vec<FrameWriteReport>,
}

impl EncodedFrames {
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn into_payload(self) -> Vec<u8> {
        self.payload
    }

    pub fn reports(&self) -> &[FrameWriteReport] {
        &self.reports
    }
}

impl AsRef<[u8]> for EncodedFrames {
    fn as_ref(&self) -> &[u8] {
        self.payload()
    }
}

#[derive(Debug)]
struct PreparedCandidate {
    candidate: FrameCandidate,
    encoding: Option<FrameEncoding>,
    bytes: Vec<u8>,
    encoded_bytes: usize,
    reconstructed: Option<Vec<u8>>,
    layout: PreparedLayout,
}

#[derive(Debug)]
enum PreparedLayout {
    Whole,
    Sparse {
        tile_width: u32,
        tile_height: u32,
        selection: GroupSelection,
        index_encoding: Option<UnitIndexEncoding>,
        index: Vec<u8>,
    },
}

#[derive(Debug)]
struct ChangedTiles {
    grid: TileGrid,
    cells: Vec<u32>,
    offsets: Vec<u32>,
    current: Vec<u8>,
    previous: Vec<u8>,
}

impl ChangedTiles {
    fn collect(
        surface: SurfaceDescriptor,
        grid: TileGrid,
        current: &[u8],
        previous: &[u8],
    ) -> Result<Self, FrameWriteError> {
        let current_surface = TightSurface::new(surface, current)?;
        let previous_surface = TightSurface::new(surface, previous)?;
        let mut changed = Self {
            grid,
            cells: Vec::new(),
            offsets: alloc::vec![0],
            current: Vec::new(),
            previous: Vec::new(),
        };
        let mut current_tile = Vec::new();
        let mut previous_tile = Vec::new();
        for (cell, region) in grid.iter().enumerate() {
            current_surface.copy_region(region, &mut current_tile)?;
            previous_surface.copy_region(region, &mut previous_tile)?;
            if current_tile != previous_tile {
                changed
                    .cells
                    .try_reserve(1)
                    .map_err(|_| FrameWriteError::AllocationFailed)?;
                changed
                    .offsets
                    .try_reserve(1)
                    .map_err(|_| FrameWriteError::AllocationFailed)?;
                changed
                    .current
                    .try_reserve(current_tile.len())
                    .map_err(|_| FrameWriteError::AllocationFailed)?;
                changed
                    .previous
                    .try_reserve(previous_tile.len())
                    .map_err(|_| FrameWriteError::AllocationFailed)?;
                changed
                    .cells
                    .push(u32::try_from(cell).map_err(|_| FrameWriteError::SizeOverflow)?);
                changed.current.extend_from_slice(&current_tile);
                changed.previous.extend_from_slice(&previous_tile);
                changed.offsets.push(
                    u32::try_from(changed.current.len())
                        .map_err(|_| FrameWriteError::SizeOverflow)?,
                );
            }
        }
        Ok(changed)
    }

    fn len(&self) -> usize {
        self.cells.len()
    }

    fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    fn region(&self, index: usize) -> Region {
        self.grid
            .get(self.cells[index] as usize)
            .expect("collected tile remains in grid")
    }

    fn current(&self, index: usize) -> &[u8] {
        &self.current[self.range(index)]
    }

    fn previous(&self, index: usize) -> &[u8] {
        &self.previous[self.range(index)]
    }

    fn range(&self, index: usize) -> core::ops::Range<usize> {
        self.offsets[index] as usize..self.offsets[index + 1] as usize
    }
}

/// Host-side builder for canonical sectioned FRAMES payloads.
///
/// Input frames use the surface's tight plane order. The previous reconstructed
/// frame and LZ4 workspace are reused across calls; the finished payload remains
/// suitable for allocation-free readers and caller-aligned playback.
#[derive(Debug)]
pub struct FramesEncoder {
    sequence: FrameSequence,
    surface: SurfaceDescriptor,
    profiles: FrameEncodingSet,
    selector: FrameSelector,
    input_alignment: ByteAlignment,
    tile_size: Option<(u32, u32)>,
    sample_bytes: usize,
    previous: Vec<u8>,
    codings: Vec<FrameEncoding>,
    groups: Vec<UnitGroupRecord>,
    frame_group_counts: Vec<u32>,
    durations: Vec<u32>,
    keyframes: Vec<u32>,
    data: Vec<u8>,
    indexes: Vec<u8>,
    color_table: Vec<u8>,
    reports: Vec<FrameWriteReport>,
    candidates: Vec<PreparedCandidate>,
    lz4_table: Vec<u32>,
}

impl FramesEncoder {
    pub fn new(
        sequence: FrameSequence,
        surface: SurfaceDescriptor,
    ) -> Result<Self, FrameWriteError> {
        let sample_bytes = tight_sample_bytes(surface)?;
        let mut previous = Vec::new();
        previous
            .try_reserve_exact(sample_bytes)
            .map_err(|_| FrameWriteError::AllocationFailed)?;
        let mut lz4_table = Vec::new();
        lz4_table
            .try_reserve_exact(Lz4::TABLE_LEN)
            .map_err(|_| FrameWriteError::AllocationFailed)?;
        lz4_table.resize(Lz4::TABLE_LEN, 0);
        let mut candidates = Vec::new();
        candidates
            .try_reserve_exact(16)
            .map_err(|_| FrameWriteError::AllocationFailed)?;
        Ok(Self {
            sequence,
            surface,
            profiles: FrameEncodingSet::default(),
            selector: FrameSelector::new(FramePolicy::new(sequence.max_delta_frames())),
            input_alignment: ByteAlignment::ONE,
            tile_size: Some((32, 32)),
            sample_bytes,
            previous,
            codings: Vec::new(),
            groups: Vec::new(),
            frame_group_counts: Vec::new(),
            durations: Vec::new(),
            keyframes: Vec::new(),
            data: Vec::new(),
            indexes: Vec::new(),
            color_table: Vec::new(),
            reports: Vec::new(),
            candidates,
            lz4_table,
        })
    }

    pub fn with_profiles(mut self, profiles: FrameEncodingSet) -> Result<Self, FrameWriteError> {
        self.ensure_not_started()?;
        self.profiles = profiles;
        Ok(self)
    }

    pub fn with_policy(mut self, policy: FramePolicy) -> Result<Self, FrameWriteError> {
        self.ensure_not_started()?;
        if policy.max_delta_frames() > self.sequence.max_delta_frames() {
            return Err(FrameWriteError::DeltaPolicyExceedsSequence {
                policy: policy.max_delta_frames(),
                sequence: self.sequence.max_delta_frames(),
            });
        }
        self.selector = FrameSelector::new(policy);
        Ok(self)
    }

    /// Aligns every encoded unit start within DATA.
    pub fn with_input_alignment(
        mut self,
        alignment: ByteAlignment,
    ) -> Result<Self, FrameWriteError> {
        self.ensure_not_started()?;
        self.input_alignment = alignment;
        Ok(self)
    }

    /// Enables sparse frame candidates over a shared surface tile grid.
    pub fn with_tiles(mut self, width: u32, height: u32) -> Result<Self, FrameWriteError> {
        self.ensure_not_started()?;
        self.surface.tile_grid(width, height)?;
        self.tile_size = Some((width, height));
        Ok(self)
    }

    /// Disables sparse frame candidates while retaining whole-frame profiles.
    pub fn without_tiles(mut self) -> Result<Self, FrameWriteError> {
        self.ensure_not_started()?;
        self.tile_size = None;
        Ok(self)
    }

    /// Stores the exact RGBA color table required by indexed sample layouts.
    pub fn with_color_table(mut self, rgba: &[u8]) -> Result<Self, FrameWriteError> {
        self.ensure_not_started()?;
        let expected = self
            .surface
            .sample_layout()
            .color_table_entries()
            .map_or(0, |entries| entries as usize * 4);
        if rgba.len() != expected {
            return Err(FrameWriteError::ColorTableLengthMismatch {
                expected,
                actual: rgba.len(),
            });
        }
        self.color_table.clear();
        self.color_table
            .try_reserve_exact(rgba.len())
            .map_err(|_| FrameWriteError::AllocationFailed)?;
        self.color_table.extend_from_slice(rgba);
        Ok(self)
    }

    pub const fn frame_count(&self) -> u32 {
        self.selector.frame()
    }

    pub const fn sample_bytes(&self) -> usize {
        self.sample_bytes
    }

    pub fn reports(&self) -> &[FrameWriteReport] {
        &self.reports
    }

    fn ensure_not_started(&self) -> Result<(), FrameWriteError> {
        if self.selector.frame() == 0 {
            Ok(())
        } else {
            Err(FrameWriteError::AlreadyStarted)
        }
    }

    /// Selects and stores one frame atomically.
    pub fn push(&mut self, samples: &[u8]) -> Result<FrameWriteReport, FrameWriteError> {
        self.push_with_duration(samples, self.sequence.default_duration_ticks())
    }

    /// Selects and stores one frame with an explicit duration in sequence ticks.
    pub fn push_with_duration(
        &mut self,
        samples: &[u8],
        duration_ticks: u32,
    ) -> Result<FrameWriteReport, FrameWriteError> {
        if self.selector.frame() >= self.sequence.frame_count() {
            return Err(FrameWriteError::TooManyFrames {
                expected: self.sequence.frame_count(),
            });
        }
        if samples.len() != self.sample_bytes {
            return Err(FrameWriteError::SampleLengthMismatch {
                expected: self.sample_bytes,
                actual: samples.len(),
            });
        }
        if duration_ticks == 0 {
            return Err(FrameWriteError::Timing(FrameTimingError::ZeroDuration {
                frame: self.selector.frame(),
            }));
        }

        self.prepare_candidates(samples)?;
        let mut offered = [FrameCandidate::omitted(); 16];
        for (slot, prepared) in offered.iter_mut().zip(&self.candidates) {
            *slot = prepared.candidate;
        }
        let mut next_selector = self.selector;
        let choice = next_selector.select(&offered[..self.candidates.len()])?;
        let selected = self
            .candidates
            .iter()
            .position(|prepared| prepared.candidate == choice.candidate())
            .expect("selected offered candidate");
        let prepared = self.candidates.swap_remove(selected);
        let report = self.commit(choice, prepared, samples, duration_ticks)?;
        self.selector = next_selector;
        self.candidates.clear();
        Ok(report)
    }

    pub fn finish(self) -> Result<EncodedFrames, FrameWriteError> {
        if self.selector.frame() != self.sequence.frame_count() {
            return Err(FrameWriteError::FrameCountMismatch {
                expected: self.sequence.frame_count(),
                actual: self.selector.frame(),
            });
        }
        let records: Vec<_> = self.codings.iter().map(FrameEncoding::record).collect();
        let mut asset = FramesAsset::from_records(
            self.sequence,
            self.surface,
            &records,
            &self.groups,
            &self.frame_group_counts,
            &self.indexes,
            &self.data,
        )?;
        if !self.color_table.is_empty() {
            asset = asset.with_color_table(&self.color_table);
        }
        let asset = asset.with_durations(&self.durations)?;
        let asset = asset.with_keyframes(&self.keyframes)?;
        Ok(EncodedFrames {
            payload: asset.encode()?,
            reports: self.reports,
        })
    }

    fn prepare_candidates(&mut self, samples: &[u8]) -> Result<(), FrameWriteError> {
        self.candidates.clear();
        if !self.previous.is_empty() && self.previous == samples {
            self.candidates.push(PreparedCandidate {
                candidate: FrameCandidate::omitted(),
                encoding: None,
                bytes: Vec::new(),
                encoded_bytes: 0,
                reconstructed: None,
                layout: PreparedLayout::Whole,
            });
        }
        if self.profiles.raw {
            let mut bytes = allocated(samples.len())?;
            bytes.copy_from_slice(samples);
            self.add_candidate(FrameStorage::Keyframe, FrameEncoding::Raw, bytes, None)?;
        }
        if self.profiles.rle {
            let codec = Rle::new();
            let mut bytes = allocated(codec.encoded_len(samples)?)?;
            let len = codec.encode_into(samples, &mut bytes)?;
            bytes.truncate(len);
            self.add_candidate(FrameStorage::Keyframe, FrameEncoding::Rle, bytes, None)?;
        }
        if self.profiles.pixel
            && matches!(
                self.surface.sample_layout(),
                crate::image::SampleLayout::RGB888 | crate::image::SampleLayout::RGBA8888
            )
        {
            let codec = Pixel::new(self.surface.sample_layout())?;
            let mut bytes = allocated(codec.encoded_len(samples)?)?;
            let len = codec.encode_into(samples, &mut bytes)?;
            bytes.truncate(len);
            self.add_candidate(FrameStorage::Keyframe, FrameEncoding::Pixel, bytes, None)?;
        }
        if self.profiles.lz4 {
            let mut encoder = Lz4::new().encoder(&mut self.lz4_table)?;
            let mut bytes = allocated(encoder.encoded_len(samples)?)?;
            let len = encoder.encode_into(samples, &mut bytes)?;
            bytes.truncate(len);
            self.add_candidate(FrameStorage::Keyframe, FrameEncoding::Lz4, bytes, None)?;
        }
        if self.profiles.frequency_reversible && frequency_supported(self.surface) {
            let (bytes, _) = encode_frequency(Frequency::reversible(), self.surface, samples)?;
            self.add_candidate(
                FrameStorage::Keyframe,
                FrameEncoding::FrequencyReversible,
                bytes,
                None,
            )?;
        }
        if let Some(quality) = self.profiles.frequency_quality {
            if frequency_supported(self.surface) {
                let codec = Frequency::quantized(quality)?;
                let (bytes, reconstructed) = encode_frequency(codec, self.surface, samples)?;
                self.add_candidate(
                    FrameStorage::Keyframe,
                    FrameEncoding::FrequencyQuantized(quality),
                    bytes,
                    reconstructed,
                )?;
            }
        }
        if self.profiles.delta && !self.previous.is_empty() {
            let codec = FrameDelta::new();
            let mut bytes = allocated(codec.encoded_len(&self.previous, samples)?)?;
            let len = codec.encode_into(&self.previous, samples, &mut bytes)?;
            bytes.truncate(len);
            self.add_candidate(FrameStorage::Delta, FrameEncoding::Delta, bytes, None)?;
        }
        if let Some((tile_width, tile_height)) = self.tile_size {
            if !self.previous.is_empty() {
                let grid = self.surface.tile_grid(tile_width, tile_height)?;
                let changed = ChangedTiles::collect(self.surface, grid, samples, &self.previous)?;
                if !changed.is_empty() {
                    self.prepare_sparse_candidates(&changed)?;
                }
            }
        }
        Ok(())
    }

    fn add_candidate(
        &mut self,
        storage: FrameStorage,
        encoding: FrameEncoding,
        bytes: Vec<u8>,
        reconstructed: Option<Vec<u8>>,
    ) -> Result<(), FrameWriteError> {
        let padding = aligned_padding(self.data.len(), self.input_alignment)?;
        let setup = if self.codings.contains(&encoding) {
            0
        } else {
            CODING_RECORD_LEN + encoding.record().params().len()
        };
        let stored = padding
            .checked_add(bytes.len())
            .and_then(|value| value.checked_add(UNIT_GROUP_RECORD_LEN))
            .and_then(|value| value.checked_add(setup))
            .ok_or(FrameWriteError::SizeOverflow)? as u64;
        let mut candidate = match storage {
            FrameStorage::Keyframe => FrameCandidate::keyframe(encoding.coding_id(), stored),
            FrameStorage::Delta => FrameCandidate::delta(stored),
            FrameStorage::Sparse => FrameCandidate::sparse(encoding.coding_id(), stored),
            FrameStorage::Omitted => FrameCandidate::omitted(),
        }
        .with_decode_cost(self.sample_bytes as u64, self.sample_bytes as u64);
        if !encoding.is_lossless() {
            candidate = candidate.lossy();
        }
        self.candidates.push(PreparedCandidate {
            candidate,
            encoding: Some(encoding),
            encoded_bytes: bytes.len(),
            bytes,
            reconstructed,
            layout: PreparedLayout::Whole,
        });
        Ok(())
    }

    fn prepare_sparse_candidates(&mut self, changed: &ChangedTiles) -> Result<(), FrameWriteError> {
        if changed.len() == changed.grid.len() {
            return Ok(());
        }
        if self.profiles.raw {
            self.add_sparse_candidate(changed, FrameEncoding::Raw)?;
        }
        if self.profiles.rle {
            self.add_sparse_candidate(changed, FrameEncoding::Rle)?;
        }
        if self.profiles.pixel
            && matches!(
                self.surface.sample_layout(),
                crate::image::SampleLayout::RGB888 | crate::image::SampleLayout::RGBA8888
            )
        {
            self.add_sparse_candidate(changed, FrameEncoding::Pixel)?;
        }
        if self.profiles.lz4 {
            self.add_sparse_candidate(changed, FrameEncoding::Lz4)?;
        }
        if self.profiles.frequency_reversible
            && changed
                .grid
                .iter()
                .all(|region| frequency_region_supported(self.surface, region))
        {
            self.add_sparse_candidate(changed, FrameEncoding::FrequencyReversible)?;
        }
        if let Some(quality) = self.profiles.frequency_quality {
            if changed
                .grid
                .iter()
                .all(|region| frequency_region_supported(self.surface, region))
            {
                self.add_sparse_candidate(changed, FrameEncoding::FrequencyQuantized(quality))?;
            }
        }
        if self.profiles.delta {
            self.add_sparse_candidate(changed, FrameEncoding::Delta)?;
        }
        Ok(())
    }

    fn add_sparse_candidate(
        &mut self,
        changed: &ChangedTiles,
        encoding: FrameEncoding,
    ) -> Result<(), FrameWriteError> {
        let mut bytes = Vec::new();
        let mut lengths = Vec::new();
        lengths
            .try_reserve_exact(changed.len())
            .map_err(|_| FrameWriteError::AllocationFailed)?;
        let mut encoded_bytes = 0usize;
        let mut decoded_bytes = 0u64;
        let mut workspace_bytes = 0u64;
        let mut reconstructed = if encoding.is_lossless() {
            None
        } else {
            let mut output = allocated(self.previous.len())?;
            output.copy_from_slice(&self.previous);
            Some(output)
        };

        for index in 0..changed.len() {
            let region = changed.region(index);
            let (encoded, decoded) = self.encode_sparse_unit(
                encoding,
                region,
                changed.current(index),
                changed.previous(index),
            )?;
            let padding = aligned_padding(bytes.len(), self.input_alignment)?;
            bytes
                .try_reserve(padding + encoded.len())
                .map_err(|_| FrameWriteError::AllocationFailed)?;
            bytes.resize(bytes.len() + padding, 0);
            bytes.extend_from_slice(&encoded);
            lengths.push(u32::try_from(encoded.len()).map_err(|_| FrameWriteError::SizeOverflow)?);
            encoded_bytes = encoded_bytes
                .checked_add(encoded.len())
                .ok_or(FrameWriteError::SizeOverflow)?;
            decoded_bytes = decoded_bytes
                .checked_add(changed.current(index).len() as u64)
                .ok_or(FrameWriteError::SizeOverflow)?;
            workspace_bytes = workspace_bytes.max(changed.current(index).len() as u64);
            if let (Some(output), Some(decoded)) = (&mut reconstructed, decoded) {
                TightSurface::replace_region(self.surface, output, region, &decoded)?;
            }
        }

        let (selection, mut index) = encode_selection(changed.grid, &changed.cells)?;
        let index_encoding = if lengths.windows(2).all(|pair| pair[0] == pair[1]) {
            None
        } else {
            let encoding = if lengths.iter().all(|&length| length <= u32::from(u16::MAX)) {
                UnitIndexEncoding::Lengths16
            } else {
                UnitIndexEncoding::Lengths32
            };
            let offset = index.len();
            let needed = encoding.encoded_len(&lengths, self.input_alignment)?;
            index
                .try_reserve(needed)
                .map_err(|_| FrameWriteError::AllocationFailed)?;
            index.resize(offset + needed, 0);
            encoding.encode_into(&lengths, self.input_alignment, &mut index[offset..])?;
            Some(encoding)
        };

        let padding = aligned_padding(self.data.len(), self.input_alignment)?;
        let setup = if self.codings.contains(&encoding) {
            0
        } else {
            CODING_RECORD_LEN + encoding.record().params().len()
        };
        let stored = padding
            .checked_add(bytes.len())
            .and_then(|value| value.checked_add(index.len()))
            .and_then(|value| value.checked_add(UNIT_GROUP_RECORD_LEN))
            .and_then(|value| value.checked_add(setup))
            .ok_or(FrameWriteError::SizeOverflow)? as u64;
        let mut candidate = FrameCandidate::sparse(encoding.coding_id(), stored)
            .with_decode_cost(decoded_bytes, workspace_bytes);
        if !encoding.is_lossless() {
            candidate = candidate.lossy();
        }
        self.candidates.push(PreparedCandidate {
            candidate,
            encoding: Some(encoding),
            bytes,
            encoded_bytes,
            reconstructed,
            layout: PreparedLayout::Sparse {
                tile_width: changed.grid.tile_width(),
                tile_height: changed.grid.tile_height(),
                selection,
                index_encoding,
                index,
            },
        });
        Ok(())
    }

    fn encode_sparse_unit(
        &mut self,
        encoding: FrameEncoding,
        region: Region,
        current: &[u8],
        previous: &[u8],
    ) -> Result<(Vec<u8>, Option<Vec<u8>>), FrameWriteError> {
        match encoding {
            FrameEncoding::Raw => {
                let mut bytes = allocated(current.len())?;
                bytes.copy_from_slice(current);
                Ok((bytes, None))
            }
            FrameEncoding::Rle => {
                let codec = Rle::new();
                let mut bytes = allocated(codec.encoded_len(current)?)?;
                let len = codec.encode_into(current, &mut bytes)?;
                bytes.truncate(len);
                Ok((bytes, None))
            }
            FrameEncoding::Pixel => {
                let codec = Pixel::new(self.surface.sample_layout())?;
                let mut bytes = allocated(codec.encoded_len(current)?)?;
                let len = codec.encode_into(current, &mut bytes)?;
                bytes.truncate(len);
                Ok((bytes, None))
            }
            FrameEncoding::Lz4 => {
                let mut encoder = Lz4::new().encoder(&mut self.lz4_table)?;
                let mut bytes = allocated(encoder.encoded_len(current)?)?;
                let len = encoder.encode_into(current, &mut bytes)?;
                bytes.truncate(len);
                Ok((bytes, None))
            }
            FrameEncoding::FrequencyReversible => {
                encode_frequency_region(Frequency::reversible(), self.surface, region, current)
            }
            FrameEncoding::FrequencyQuantized(quality) => encode_frequency_region(
                Frequency::quantized(quality)?,
                self.surface,
                region,
                current,
            ),
            FrameEncoding::Delta => {
                let codec = FrameDelta::new();
                let mut bytes = allocated(codec.encoded_len(previous, current)?)?;
                let len = codec.encode_into(previous, current, &mut bytes)?;
                bytes.truncate(len);
                Ok((bytes, None))
            }
        }
    }

    fn commit(
        &mut self,
        choice: FrameChoice,
        prepared: PreparedCandidate,
        samples: &[u8],
        duration_ticks: u32,
    ) -> Result<FrameWriteReport, FrameWriteError> {
        let encoded_bytes =
            u32::try_from(prepared.encoded_bytes).map_err(|_| FrameWriteError::SizeOverflow)?;
        self.durations
            .try_reserve(1)
            .map_err(|_| FrameWriteError::AllocationFailed)?;
        self.reports
            .try_reserve(1)
            .map_err(|_| FrameWriteError::AllocationFailed)?;
        if let Some(encoding) = prepared.encoding {
            let existing_coding = self.codings.iter().position(|stored| *stored == encoding);
            let coding_index = existing_coding.unwrap_or(self.codings.len());
            let padding = aligned_padding(self.data.len(), self.input_alignment)?;
            let final_len = self
                .data
                .len()
                .checked_add(padding)
                .and_then(|len| len.checked_add(prepared.bytes.len()))
                .ok_or(FrameWriteError::SizeOverflow)?;
            let start = u32::try_from(self.data.len() + padding)
                .map_err(|_| FrameWriteError::SizeOverflow)?;
            let end = u32::try_from(final_len).map_err(|_| FrameWriteError::SizeOverflow)?;
            let mut group = UnitGroupRecord::new(
                u32::try_from(coding_index).map_err(|_| FrameWriteError::SizeOverflow)?,
                start..end,
            )?
            .with_input_alignment(self.input_alignment);
            let mut candidate_index = None;
            match &prepared.layout {
                PreparedLayout::Whole => {}
                PreparedLayout::Sparse {
                    tile_width,
                    tile_height,
                    selection,
                    index_encoding,
                    index,
                } => {
                    let index_offset = u32::try_from(self.indexes.len())
                        .map_err(|_| FrameWriteError::SizeOverflow)?;
                    group = group
                        .with_tiles(*tile_width, *tile_height)
                        .with_selection(*selection)
                        .with_index_offset(index_offset);
                    if let Some(encoding) = index_encoding {
                        group = group.with_index_encoding(*encoding);
                    }
                    candidate_index = Some(index);
                }
            }
            if matches!(
                choice.candidate().storage(),
                FrameStorage::Sparse | FrameStorage::Delta
            ) {
                group = group.with_reference(ReferenceMode::Previous);
            }

            if existing_coding.is_none() {
                self.codings
                    .try_reserve(1)
                    .map_err(|_| FrameWriteError::AllocationFailed)?;
            }
            self.data
                .try_reserve(padding + prepared.bytes.len())
                .map_err(|_| FrameWriteError::AllocationFailed)?;
            if let Some(index) = candidate_index {
                self.indexes
                    .try_reserve(index.len())
                    .map_err(|_| FrameWriteError::AllocationFailed)?;
            }
            self.groups
                .try_reserve(1)
                .map_err(|_| FrameWriteError::AllocationFailed)?;
            self.frame_group_counts
                .try_reserve(1)
                .map_err(|_| FrameWriteError::AllocationFailed)?;
            if choice.candidate().storage().is_independent() {
                self.keyframes
                    .try_reserve(1)
                    .map_err(|_| FrameWriteError::AllocationFailed)?;
            }

            if existing_coding.is_none() {
                self.codings.push(encoding);
            }
            self.data.resize(self.data.len() + padding, 0);
            self.data.extend_from_slice(&prepared.bytes);
            if let Some(index) = candidate_index {
                self.indexes.extend_from_slice(index);
            }
            self.groups.push(group);
            self.frame_group_counts.push(1);
            if choice.candidate().storage().is_independent() {
                self.keyframes.push(choice.frame());
            }
        } else {
            self.frame_group_counts
                .try_reserve(1)
                .map_err(|_| FrameWriteError::AllocationFailed)?;
            self.frame_group_counts.push(0);
        }
        self.previous.clear();
        self.previous
            .extend_from_slice(prepared.reconstructed.as_deref().unwrap_or(samples));
        let report = FrameWriteReport {
            frame: choice.frame(),
            storage: choice.candidate().storage(),
            encoding: prepared.encoding,
            encoded_bytes,
            stored_bytes: choice.candidate().stored_bytes(),
            delta_frames: choice.delta_frames(),
        };
        self.durations.push(duration_ticks);
        self.reports.push(report);
        Ok(report)
    }
}

#[derive(Clone, Copy, Debug)]
struct TightSurface<'a> {
    surface: SurfaceDescriptor,
    bytes: &'a [u8],
}

impl<'a> TightSurface<'a> {
    fn new(surface: SurfaceDescriptor, bytes: &'a [u8]) -> Result<Self, FrameWriteError> {
        let expected = tight_sample_bytes(surface)?;
        if bytes.len() != expected {
            return Err(FrameWriteError::SampleLengthMismatch {
                expected,
                actual: bytes.len(),
            });
        }
        Ok(Self { surface, bytes })
    }

    fn region_byte_len(self, region: Region) -> Result<usize, FrameWriteError> {
        let mut total = 0usize;
        for index in 0..self.surface.plane_count() {
            let plane = self.surface.plane(index).expect("surface plane");
            let projected = region.for_plane(self.surface, index)?;
            let row_bits = u64::from(projected.width()) * u64::from(plane.bits_per_element());
            let row_bytes =
                usize::try_from(row_bits.div_ceil(8)).map_err(|_| FrameWriteError::SizeOverflow)?;
            total = total
                .checked_add(
                    row_bytes
                        .checked_mul(projected.height() as usize)
                        .ok_or(FrameWriteError::SizeOverflow)?,
                )
                .ok_or(FrameWriteError::SizeOverflow)?;
        }
        Ok(total)
    }

    fn copy_region(self, region: Region, output: &mut Vec<u8>) -> Result<(), FrameWriteError> {
        let needed = self.region_byte_len(region)?;
        output.clear();
        output
            .try_reserve(needed)
            .map_err(|_| FrameWriteError::AllocationFailed)?;
        output.resize(needed, 0);

        let mut plane_offset = 0usize;
        let mut output_offset = 0usize;
        for index in 0..self.surface.plane_count() {
            let plane = self.surface.plane(index).expect("surface plane");
            let projected = region.for_plane(self.surface, index)?;
            let stride = usize::try_from(
                plane
                    .minimum_stride()
                    .ok_or(FrameWriteError::SizeOverflow)?,
            )
            .map_err(|_| FrameWriteError::SizeOverflow)?;
            let plane_len = stride
                .checked_mul(plane.height() as usize)
                .ok_or(FrameWriteError::SizeOverflow)?;
            let start_bit = u64::from(projected.x()) * u64::from(plane.bits_per_element());
            let row_bits = u64::from(projected.width()) * u64::from(plane.bits_per_element());
            let row_bytes =
                usize::try_from(row_bits.div_ceil(8)).map_err(|_| FrameWriteError::SizeOverflow)?;
            for row in 0..projected.height() as usize {
                let source_offset = plane_offset
                    .checked_add((projected.y() as usize + row) * stride)
                    .and_then(|offset| offset.checked_add(start_bit as usize / 8))
                    .ok_or(FrameWriteError::SizeOverflow)?;
                crate::image::samples::copy(
                    &self.bytes[source_offset..],
                    (start_bit % 8) as u8,
                    &mut output[output_offset..],
                    0,
                    row_bits,
                );
                output_offset += row_bytes;
            }
            plane_offset += plane_len;
        }
        debug_assert_eq!(output_offset, needed);
        Ok(())
    }

    fn replace_region(
        surface: SurfaceDescriptor,
        target: &mut [u8],
        region: Region,
        source: &[u8],
    ) -> Result<(), FrameWriteError> {
        let target_len = tight_sample_bytes(surface)?;
        if target.len() != target_len {
            return Err(FrameWriteError::SampleLengthMismatch {
                expected: target_len,
                actual: target.len(),
            });
        }
        let expected = Self {
            surface,
            bytes: &[],
        }
        .region_byte_len(region)?;
        if source.len() != expected {
            return Err(FrameWriteError::SampleLengthMismatch {
                expected,
                actual: source.len(),
            });
        }

        let mut plane_offset = 0usize;
        let mut source_offset = 0usize;
        for index in 0..surface.plane_count() {
            let plane = surface.plane(index).expect("surface plane");
            let projected = region.for_plane(surface, index)?;
            let stride = usize::try_from(
                plane
                    .minimum_stride()
                    .ok_or(FrameWriteError::SizeOverflow)?,
            )
            .map_err(|_| FrameWriteError::SizeOverflow)?;
            let plane_len = stride
                .checked_mul(plane.height() as usize)
                .ok_or(FrameWriteError::SizeOverflow)?;
            let start_bit = u64::from(projected.x()) * u64::from(plane.bits_per_element());
            let row_bits = u64::from(projected.width()) * u64::from(plane.bits_per_element());
            let row_bytes =
                usize::try_from(row_bits.div_ceil(8)).map_err(|_| FrameWriteError::SizeOverflow)?;
            for row in 0..projected.height() as usize {
                let target_offset = plane_offset
                    .checked_add((projected.y() as usize + row) * stride)
                    .and_then(|offset| offset.checked_add(start_bit as usize / 8))
                    .ok_or(FrameWriteError::SizeOverflow)?;
                crate::image::samples::copy(
                    &source[source_offset..],
                    0,
                    &mut target[target_offset..],
                    (start_bit % 8) as u8,
                    row_bits,
                );
                source_offset += row_bytes;
            }
            plane_offset += plane_len;
        }
        debug_assert_eq!(source_offset, expected);
        Ok(())
    }
}

fn allocated(len: usize) -> Result<Vec<u8>, FrameWriteError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| FrameWriteError::AllocationFailed)?;
    bytes.resize(len, 0);
    Ok(bytes)
}

fn aligned_padding(offset: usize, alignment: ByteAlignment) -> Result<usize, FrameWriteError> {
    let alignment = usize::try_from(alignment.get()).expect("u32 fits usize");
    let aligned = offset
        .checked_add(alignment - 1)
        .map(|value| value & !(alignment - 1))
        .ok_or(FrameWriteError::SizeOverflow)?;
    Ok(aligned - offset)
}

fn tight_sample_bytes(surface: SurfaceDescriptor) -> Result<usize, FrameWriteError> {
    usize::try_from(
        surface
            .tight_byte_len()
            .ok_or(FrameWriteError::SizeOverflow)?,
    )
    .map_err(|_| FrameWriteError::SizeOverflow)
}

fn frequency_supported(surface: SurfaceDescriptor) -> bool {
    surface.planes().enumerate().all(|(index, plane)| {
        FrequencyGeometry::for_plane(
            surface.sample_layout(),
            index as u8,
            plane.width(),
            plane.height(),
        )
        .is_ok()
    })
}

fn frequency_region_supported(surface: SurfaceDescriptor, region: Region) -> bool {
    (0..surface.plane_count()).all(|index| {
        region.for_plane(surface, index).is_ok_and(|plane| {
            FrequencyGeometry::for_plane(
                surface.sample_layout(),
                index,
                plane.width(),
                plane.height(),
            )
            .is_ok()
        })
    })
}

fn encode_selection(
    grid: TileGrid,
    cells: &[u32],
) -> Result<(GroupSelection, Vec<u8>), FrameWriteError> {
    if cells.len() == grid.len() {
        return Ok((GroupSelection::All, Vec::new()));
    }
    let cell_count = u32::try_from(grid.len()).map_err(|_| FrameWriteError::SizeOverflow)?;
    let list_len = UnitSelectionEncoding::List.encoded_len(cell_count, cells)?;
    let bitmap_len = UnitSelectionEncoding::Bitmap.encoded_len(cell_count, cells)?;
    let (encoding, selection, len) = if list_len <= bitmap_len {
        (
            UnitSelectionEncoding::List,
            GroupSelection::List(
                u32::try_from(cells.len()).map_err(|_| FrameWriteError::SizeOverflow)?,
            ),
            list_len,
        )
    } else {
        (
            UnitSelectionEncoding::Bitmap,
            GroupSelection::Bitmap,
            bitmap_len,
        )
    };
    let mut bytes = allocated(len)?;
    encoding.encode_into(cell_count, cells, &mut bytes)?;
    Ok((selection, bytes))
}

fn encode_frequency(
    codec: Frequency,
    surface: SurfaceDescriptor,
    samples: &[u8],
) -> Result<(Vec<u8>, Option<Vec<u8>>), FrameWriteError> {
    let mut encoded_lengths = [0usize; 3];
    let mut encoded_len = 0usize;
    let mut sample_offset = 0usize;
    for (index, plane) in surface.planes().enumerate() {
        let geometry = FrequencyGeometry::for_plane(
            surface.sample_layout(),
            index as u8,
            plane.width(),
            plane.height(),
        )?;
        let sample_len = geometry.decoded_len()?;
        let sample_end = sample_offset
            .checked_add(sample_len)
            .ok_or(FrameWriteError::SizeOverflow)?;
        let len = codec.encoded_len(geometry, &samples[sample_offset..sample_end])?;
        encoded_lengths[index] = len;
        encoded_len = encoded_len
            .checked_add(len)
            .ok_or(FrameWriteError::SizeOverflow)?;
        sample_offset = sample_end;
    }
    debug_assert_eq!(sample_offset, samples.len());

    let mut encoded = allocated(encoded_len)?;
    let mut reconstructed = if codec.is_reversible() {
        None
    } else {
        Some(allocated(samples.len())?)
    };
    let mut encoded_offset = 0usize;
    sample_offset = 0;
    for (index, plane) in surface.planes().enumerate() {
        let geometry = FrequencyGeometry::for_plane(
            surface.sample_layout(),
            index as u8,
            plane.width(),
            plane.height(),
        )?;
        let sample_len = geometry.decoded_len()?;
        let sample_end = sample_offset + sample_len;
        let encoded_end = encoded_offset + encoded_lengths[index];
        codec.encode_into(
            geometry,
            &samples[sample_offset..sample_end],
            &mut encoded[encoded_offset..encoded_end],
        )?;
        if let Some(output) = &mut reconstructed {
            codec
                .plan(&encoded[encoded_offset..encoded_end], geometry)?
                .decode_into(&mut output[sample_offset..sample_end])?;
        }
        sample_offset = sample_end;
        encoded_offset = encoded_end;
    }
    Ok((encoded, reconstructed))
}

fn encode_frequency_region(
    codec: Frequency,
    surface: SurfaceDescriptor,
    region: Region,
    samples: &[u8],
) -> Result<(Vec<u8>, Option<Vec<u8>>), FrameWriteError> {
    let mut geometries = [None; 3];
    let mut encoded_lengths = [0usize; 3];
    let mut encoded_len = 0usize;
    let mut sample_offset = 0usize;
    for index in 0..surface.plane_count() {
        let plane = region.for_plane(surface, index)?;
        let geometry = FrequencyGeometry::for_plane(
            surface.sample_layout(),
            index,
            plane.width(),
            plane.height(),
        )?;
        let sample_len = geometry.decoded_len()?;
        let sample_end = sample_offset
            .checked_add(sample_len)
            .ok_or(FrameWriteError::SizeOverflow)?;
        let len = codec.encoded_len(geometry, &samples[sample_offset..sample_end])?;
        geometries[index as usize] = Some(geometry);
        encoded_lengths[index as usize] = len;
        encoded_len = encoded_len
            .checked_add(len)
            .ok_or(FrameWriteError::SizeOverflow)?;
        sample_offset = sample_end;
    }
    debug_assert_eq!(sample_offset, samples.len());

    let mut encoded = allocated(encoded_len)?;
    let mut reconstructed = if codec.is_reversible() {
        None
    } else {
        Some(allocated(samples.len())?)
    };
    let mut encoded_offset = 0usize;
    sample_offset = 0;
    for index in 0..surface.plane_count() as usize {
        let geometry = geometries[index].expect("validated region plane");
        let sample_len = geometry.decoded_len()?;
        let sample_end = sample_offset + sample_len;
        let encoded_end = encoded_offset + encoded_lengths[index];
        codec.encode_into(
            geometry,
            &samples[sample_offset..sample_end],
            &mut encoded[encoded_offset..encoded_end],
        )?;
        if let Some(output) = &mut reconstructed {
            codec
                .plan(&encoded[encoded_offset..encoded_end], geometry)?
                .decode_into(&mut output[sample_offset..sample_end])?;
        }
        sample_offset = sample_end;
        encoded_offset = encoded_end;
    }
    Ok((encoded, reconstructed))
}

/// Failure while selecting, storing, or finishing a sectioned frame sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameWriteError {
    AllocationFailed,
    SizeOverflow,
    AlreadyStarted,
    DeltaPolicyExceedsSequence { policy: u16, sequence: u16 },
    SampleLengthMismatch { expected: usize, actual: usize },
    TooManyFrames { expected: u32 },
    FrameCountMismatch { expected: u32, actual: u32 },
    ColorTableLengthMismatch { expected: usize, actual: usize },
    Candidate(FrameSelectionError),
    Rle(RleError),
    Pixel(PixelError),
    Lz4(Lz4Error),
    Delta(FrameDeltaError),
    Frequency(FrequencyError),
    Region(RegionError),
    TileGrid(TileGridError),
    UnitSelection(UnitSelectionError),
    UnitIndex(UnitIndexError),
    Group(crate::image::EncodedGroupError),
    Timing(FrameTimingError),
    Payload(FramesEncodeError),
}

impl From<FrameSelectionError> for FrameWriteError {
    fn from(error: FrameSelectionError) -> Self {
        Self::Candidate(error)
    }
}

impl From<RleError> for FrameWriteError {
    fn from(error: RleError) -> Self {
        Self::Rle(error)
    }
}

impl From<PixelError> for FrameWriteError {
    fn from(error: PixelError) -> Self {
        Self::Pixel(error)
    }
}

impl From<Lz4Error> for FrameWriteError {
    fn from(error: Lz4Error) -> Self {
        Self::Lz4(error)
    }
}

impl From<FrameDeltaError> for FrameWriteError {
    fn from(error: FrameDeltaError) -> Self {
        Self::Delta(error)
    }
}

impl From<FrequencyError> for FrameWriteError {
    fn from(error: FrequencyError) -> Self {
        Self::Frequency(error)
    }
}

impl From<RegionError> for FrameWriteError {
    fn from(error: RegionError) -> Self {
        Self::Region(error)
    }
}

impl From<TileGridError> for FrameWriteError {
    fn from(error: TileGridError) -> Self {
        Self::TileGrid(error)
    }
}

impl From<UnitSelectionError> for FrameWriteError {
    fn from(error: UnitSelectionError) -> Self {
        Self::UnitSelection(error)
    }
}

impl From<UnitIndexError> for FrameWriteError {
    fn from(error: UnitIndexError) -> Self {
        Self::UnitIndex(error)
    }
}

impl From<crate::image::EncodedGroupError> for FrameWriteError {
    fn from(error: crate::image::EncodedGroupError) -> Self {
        Self::Group(error)
    }
}

impl From<FramesEncodeError> for FrameWriteError {
    fn from(error: FramesEncodeError) -> Self {
        Self::Payload(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        PayloadLimits,
        frames::FramesView,
        image::{ColorDescription, SampleLayout, SurfaceRequirements},
    };
    use alloc::vec;

    fn surface() -> SurfaceDescriptor {
        SurfaceDescriptor::new(4, 1, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap()
    }

    #[test]
    fn selects_keyframe_omission_and_delta_then_replays_exactly() {
        let sequence = FrameSequence::new(4, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(2)
            .unwrap();
        let first = [
            10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255, 10, 20, 30, 255,
        ];
        let same = first;
        let changed = [
            11, 20, 30, 255, 11, 20, 30, 255, 11, 20, 30, 255, 11, 20, 30, 255,
        ];
        let last = [
            12, 20, 30, 255, 12, 20, 30, 255, 12, 20, 30, 255, 12, 20, 30, 255,
        ];
        let profiles = FrameEncodingSet::lossless()
            .with_rle(false)
            .with_pixel(false)
            .with_lz4(false)
            .with_reversible_frequency(false);
        let mut encoder = FramesEncoder::new(sequence, surface())
            .unwrap()
            .with_profiles(profiles)
            .unwrap();
        for frame in [&first[..], &same, &changed, &last] {
            encoder.push(frame).unwrap();
        }
        let encoded = encoder.finish().unwrap();
        assert_eq!(encoded.as_ref(), encoded.payload());
        assert_eq!(encoded.reports()[0].storage(), FrameStorage::Keyframe);
        assert_eq!(encoded.reports()[1].storage(), FrameStorage::Omitted);
        assert_eq!(encoded.reports()[2].storage(), FrameStorage::Delta);
        assert_eq!(encoded.reports()[3].storage(), FrameStorage::Keyframe);

        let frames = FramesView::open(encoded.payload(), &PayloadLimits::HOST).unwrap();
        let mut groups = [None];
        let mut canvas = [0; 16];
        let mut workspace = [0; 16];
        let mut playback = frames
            .test_session(
                SurfaceRequirements::new(),
                PayloadLimits::HOST,
                &mut groups,
                &mut canvas,
                &mut workspace,
                &mut [],
            )
            .unwrap();
        for (index, expected) in [first, same, changed, last].into_iter().enumerate() {
            let view = playback.present(index as u32).unwrap();
            assert_eq!(view.plane(0).unwrap().bytes(), &expected);
        }
    }

    #[test]
    fn explicit_frame_durations_roundtrip_and_zero_is_atomic() {
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        let first = [
            10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 1, 2, 3, 255,
        ];
        let second = [
            11, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 1, 2, 3, 255,
        ];
        let mut encoder = FramesEncoder::new(sequence, surface()).unwrap();
        assert_eq!(
            encoder.push_with_duration(&first, 0),
            Err(FrameWriteError::Timing(FrameTimingError::ZeroDuration {
                frame: 0,
            }))
        );
        assert_eq!(encoder.frame_count(), 0);
        encoder.push_with_duration(&first, 40).unwrap();
        encoder.push_with_duration(&second, 75).unwrap();

        let encoded = encoder.finish().unwrap();
        let frames = FramesView::open(encoded.payload(), &PayloadLimits::HOST).unwrap();
        assert_eq!(frames.frame(0).unwrap().duration_ticks(), 40);
        assert_eq!(frames.frame(1).unwrap().duration_ticks(), 75);
    }

    #[test]
    fn aligns_group_starts_and_reports_complete_incremental_bytes() {
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        let profiles = FrameEncodingSet::lossless()
            .with_rle(false)
            .with_pixel(false)
            .with_lz4(false)
            .with_reversible_frequency(false)
            .with_delta(false);
        let mut encoder = FramesEncoder::new(sequence, surface())
            .unwrap()
            .with_profiles(profiles)
            .unwrap()
            .with_input_alignment(crate::ByteAlignment::new(64).unwrap())
            .unwrap();
        encoder.push(&[1; 16]).unwrap();
        encoder.push(&[2; 16]).unwrap();
        let encoded = encoder.finish().unwrap();
        assert_eq!(encoded.reports()[0].stored_bytes(), 16 + 36 + 8);
        assert_eq!(encoded.reports()[1].stored_bytes(), 48 + 16 + 36);
        #[repr(align(64))]
        struct Aligned([u8; 1024]);
        let mut aligned = Aligned([0; 1024]);
        aligned.0[..encoded.payload().len()].copy_from_slice(encoded.payload());
        let frames = FramesView::open_at(
            &aligned.0[..encoded.payload().len()],
            0,
            &PayloadLimits::HOST,
        )
        .unwrap();
        let mut slots = [None];
        let groups = frames
            .groups_into(
                1,
                &mut slots,
                &mut crate::image::CoverageBudget::new(u64::MAX),
            )
            .unwrap();
        assert_eq!(groups.len(), 1);
        assert!(slots[0].unwrap().get(0).unwrap().data_address_is_aligned());
    }

    #[test]
    fn validates_lengths_counts_and_policy_before_mutating_output() {
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        assert!(matches!(
            FramesEncoder::new(sequence, surface())
                .unwrap()
                .with_policy(FramePolicy::new(2)),
            Err(FrameWriteError::DeltaPolicyExceedsSequence { .. })
        ));
        let mut encoder = FramesEncoder::new(sequence, surface()).unwrap();
        assert_eq!(
            encoder.push(&[0; 15]),
            Err(FrameWriteError::SampleLengthMismatch {
                expected: 16,
                actual: 15
            })
        );
        assert_eq!(encoder.frame_count(), 0);
        encoder.push(&[0; 16]).unwrap();
        assert!(matches!(
            encoder.finish(),
            Err(FrameWriteError::FrameCountMismatch {
                expected: 2,
                actual: 1
            })
        ));
    }

    #[test]
    fn quantized_history_is_reconstructed_before_the_next_delta() {
        let surface =
            SurfaceDescriptor::new(8, 8, SampleLayout::L8, ColorDescription::SRGB).unwrap();
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        let profiles = FrameEncodingSet::lossless()
            .with_raw(false)
            .with_rle(false)
            .with_pixel(false)
            .with_lz4(false)
            .with_reversible_frequency(false)
            .with_quantized_frequency(10)
            .unwrap();
        let source = core::array::from_fn::<_, 64, _>(|index| (index * 37 + 11) as u8);
        let (_, reconstructed) =
            encode_frequency(Frequency::quantized(10).unwrap(), surface, &source).unwrap();
        let second: Vec<_> = reconstructed
            .unwrap()
            .into_iter()
            .map(|sample| sample.wrapping_add(1))
            .collect();
        let mut encoder = FramesEncoder::new(sequence, surface)
            .unwrap()
            .with_profiles(profiles)
            .unwrap()
            .with_policy(FramePolicy::new(1).allow_lossy())
            .unwrap();
        encoder.push(&source).unwrap();
        encoder.push(&second).unwrap();
        let encoded = encoder.finish().unwrap();
        assert_eq!(
            encoded.reports()[0].encoding(),
            Some(FrameEncoding::FrequencyQuantized(10))
        );
        assert_eq!(encoded.reports()[1].encoding(), Some(FrameEncoding::Delta));

        let frames = FramesView::open(encoded.payload(), &PayloadLimits::HOST).unwrap();
        let mut groups = [None];
        let mut canvas = [0; 64];
        let mut workspace = [0; 64];
        let mut playback = frames
            .test_session(
                SurfaceRequirements::new(),
                PayloadLimits::HOST,
                &mut groups,
                &mut canvas,
                &mut workspace,
                &mut [],
            )
            .unwrap();
        let first = playback.present(0).unwrap().plane(0).unwrap().bytes();
        assert_ne!(first, source);
        assert_eq!(
            playback.present(1).unwrap().plane(0).unwrap().bytes(),
            &second
        );
    }

    #[test]
    fn quantized_only_profile_requires_explicit_loss_policy() {
        let surface =
            SurfaceDescriptor::new(8, 8, SampleLayout::L8, ColorDescription::SRGB).unwrap();
        let profiles = FrameEncodingSet::lossless()
            .with_raw(false)
            .with_rle(false)
            .with_pixel(false)
            .with_lz4(false)
            .with_reversible_frequency(false)
            .with_delta(false)
            .with_quantized_frequency(50)
            .unwrap();
        let mut encoder = FramesEncoder::new(FrameSequence::new(1, 1_000, 40).unwrap(), surface)
            .unwrap()
            .with_profiles(profiles)
            .unwrap();
        assert_eq!(
            encoder.push(&[127; 64]),
            Err(FrameWriteError::Candidate(
                FrameSelectionError::NoCandidate { frame: 0 }
            ))
        );
        assert_eq!(encoder.frame_count(), 0);
    }

    #[test]
    fn reversible_frequency_roundtrips_joint_yuv_planes() {
        let surface = SurfaceDescriptor::new(
            4,
            4,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let sequence = FrameSequence::new(1, 1_000, 40).unwrap();
        let profiles = FrameEncodingSet::lossless()
            .with_raw(false)
            .with_rle(false)
            .with_pixel(false)
            .with_lz4(false)
            .with_delta(false);
        let source = core::array::from_fn::<_, 24, _>(|index| (index * 19 + 7) as u8);
        let mut encoder = FramesEncoder::new(sequence, surface)
            .unwrap()
            .with_profiles(profiles)
            .unwrap();
        encoder.push(&source).unwrap();
        let encoded = encoder.finish().unwrap();
        assert_eq!(
            encoded.reports()[0].encoding(),
            Some(FrameEncoding::FrequencyReversible)
        );

        let frames = FramesView::open(encoded.payload(), &PayloadLimits::HOST).unwrap();
        let mut groups = [None];
        let mut canvas = [0; 24];
        let mut workspace = [0; 24];
        let mut playback = frames
            .test_session(
                SurfaceRequirements::new(),
                PayloadLimits::HOST,
                &mut groups,
                &mut canvas,
                &mut workspace,
                &mut [],
            )
            .unwrap();
        let view = playback.present(0).unwrap();
        let decoded: Vec<_> = view
            .planes()
            .flat_map(|plane| plane.rows().unwrap())
            .flatten()
            .copied()
            .collect();
        assert_eq!(decoded, source);
    }

    #[test]
    fn sparse_raw_frame_stores_only_changed_tiles_and_replays_exactly() {
        let surface =
            SurfaceDescriptor::new(64, 64, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        let profiles = raw_only_profiles();
        let first = vec![0; 64 * 64 * 4];
        let mut second = first.clone();
        let changed = surface.region(16, 32, 16, 16).unwrap();
        TightSurface::replace_region(surface, &mut second, changed, &[0x5a; 16 * 16 * 4]).unwrap();
        let mut encoder = FramesEncoder::new(sequence, surface)
            .unwrap()
            .with_profiles(profiles)
            .unwrap()
            .with_tiles(16, 16)
            .unwrap();
        encoder.push(&first).unwrap();
        let report = encoder.push(&second).unwrap();
        assert_eq!(report.storage(), FrameStorage::Sparse);
        assert_eq!(report.encoding(), Some(FrameEncoding::Raw));
        assert_eq!(report.encoded_bytes(), 16 * 16 * 4);
        let encoded = encoder.finish().unwrap();

        let frames = FramesView::open(encoded.payload(), &PayloadLimits::HOST).unwrap();
        let mut slots = [None];
        let groups = frames
            .groups_into(
                1,
                &mut slots,
                &mut crate::image::CoverageBudget::new(u64::MAX),
            )
            .unwrap();
        let group = groups.iter().next().unwrap();
        assert_eq!(group.len(), 1);
        assert_eq!(group.grid().tile_width(), 16);
        assert_eq!(group.get(0).unwrap().cell(), 9);

        let mut canvas = vec![0; second.len()];
        let mut workspace = vec![0; second.len()];
        let mut playback = frames
            .test_session(
                SurfaceRequirements::new(),
                PayloadLimits::HOST,
                &mut slots,
                &mut canvas,
                &mut workspace,
                &mut [],
            )
            .unwrap();
        playback.present(0).unwrap();
        assert_eq!(tight_frame(playback.present(1).unwrap()), second);
    }

    #[test]
    fn sparse_edge_tiles_use_compact_lengths_and_aligned_unit_starts() {
        let surface =
            SurfaceDescriptor::new(40, 8, SampleLayout::L8, ColorDescription::SRGB).unwrap();
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        let first = [0; 320];
        let mut second = first;
        TightSurface::replace_region(
            surface,
            &mut second,
            surface.region(0, 0, 16, 8).unwrap(),
            &[1; 128],
        )
        .unwrap();
        TightSurface::replace_region(
            surface,
            &mut second,
            surface.region(32, 0, 8, 8).unwrap(),
            &[2; 64],
        )
        .unwrap();
        let mut encoder = FramesEncoder::new(sequence, surface)
            .unwrap()
            .with_profiles(raw_only_profiles())
            .unwrap()
            .with_tiles(16, 8)
            .unwrap()
            .with_input_alignment(crate::ByteAlignment::new(64).unwrap())
            .unwrap();
        encoder.push(&first).unwrap();
        assert_eq!(
            encoder.push(&second).unwrap().storage(),
            FrameStorage::Sparse
        );
        let encoded = encoder.finish().unwrap();

        #[repr(align(64))]
        struct Aligned([u8; 2048]);
        let mut aligned = Aligned([0; 2048]);
        aligned.0[..encoded.payload().len()].copy_from_slice(encoded.payload());
        let frames = FramesView::open_at(
            &aligned.0[..encoded.payload().len()],
            0,
            &PayloadLimits::HOST,
        )
        .unwrap();
        let mut slots = [None];
        let groups = frames
            .groups_into(
                1,
                &mut slots,
                &mut crate::image::CoverageBudget::new(u64::MAX),
            )
            .unwrap();
        let group = groups.iter().next().unwrap();
        assert_eq!(group.len(), 2);
        assert!(group.data_addresses_are_aligned());
        assert_eq!(group.get(0).unwrap().data().len(), 128);
        assert_eq!(group.get(1).unwrap().data().len(), 64);
        assert!(group.iter().all(|unit| unit.data_address_is_aligned()));
    }

    #[test]
    fn sparse_regions_preserve_joint_yuv_edges_and_packed_neighbours() {
        let nv12 = SurfaceDescriptor::new(
            5,
            3,
            SampleLayout::NV12,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let first = [0; 27];
        let mut second = first;
        TightSurface::replace_region(
            nv12,
            &mut second,
            nv12.region(4, 2, 1, 1).unwrap(),
            &[7, 8, 9],
        )
        .unwrap();
        sparse_raw_roundtrip(nv12, 4, 2, &first, &second);

        let indexed =
            SurfaceDescriptor::new(17, 2, SampleLayout::I1, ColorDescription::SRGB).unwrap();
        let first = [0b1111_0000, 0, 0, 0b0000_1111, 0, 0];
        let mut second = first;
        TightSurface::replace_region(
            indexed,
            &mut second,
            indexed.region(8, 1, 8, 1).unwrap(),
            &[0b1010_1010],
        )
        .unwrap();
        assert_eq!(second[3], first[3]);
        assert_eq!(second[4], 0b1010_1010);
        sparse_raw_roundtrip(indexed, 8, 1, &first, &second);
    }

    #[test]
    fn quantized_sparse_history_tracks_the_reconstructed_tiles() {
        let surface =
            SurfaceDescriptor::new(32, 16, SampleLayout::L8, ColorDescription::SRGB).unwrap();
        let sequence = FrameSequence::new(3, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(2)
            .unwrap();
        let profiles = FrameEncodingSet::lossless()
            .with_raw(false)
            .with_rle(false)
            .with_pixel(false)
            .with_lz4(false)
            .with_reversible_frequency(false)
            .with_delta(false)
            .with_quantized_frequency(35)
            .unwrap();
        let first = core::array::from_fn::<_, 512, _>(|index| (index * 29 + 7) as u8);
        let mut encoder = FramesEncoder::new(sequence, surface)
            .unwrap()
            .with_profiles(profiles)
            .unwrap()
            .with_policy(FramePolicy::new(2).allow_lossy())
            .unwrap()
            .with_tiles(16, 8)
            .unwrap();
        encoder.push(&first).unwrap();
        let mut second = encoder.previous.clone();
        TightSurface::replace_region(
            surface,
            &mut second,
            surface.region(16, 8, 16, 8).unwrap(),
            &core::array::from_fn::<_, 128, _>(|index| (index * 47 + 13) as u8),
        )
        .unwrap();
        let report = encoder.push(&second).unwrap();
        assert_eq!(report.storage(), FrameStorage::Sparse);
        assert_eq!(
            report.encoding(),
            Some(FrameEncoding::FrequencyQuantized(35))
        );
        assert_ne!(encoder.previous, second);
        let reconstructed = encoder.previous.clone();
        assert_eq!(
            encoder.push(&reconstructed).unwrap().storage(),
            FrameStorage::Omitted
        );
    }

    #[test]
    fn every_lossless_sparse_profile_replays_the_selected_tiles() {
        let surface =
            SurfaceDescriptor::new(64, 64, SampleLayout::RGBA8888, ColorDescription::SRGB).unwrap();
        let first: Vec<_> = (0..64 * 64 * 4)
            .map(|index| (index * 73 + index / 11 + 19) as u8)
            .collect();
        let mut second = first.clone();
        TightSurface::replace_region(
            surface,
            &mut second,
            surface.region(32, 16, 16, 16).unwrap(),
            &core::array::from_fn::<_, 1024, _>(|index| (index * 31 + 5) as u8),
        )
        .unwrap();

        for encoding in [
            FrameEncoding::Rle,
            FrameEncoding::Pixel,
            FrameEncoding::Lz4,
            FrameEncoding::FrequencyReversible,
            FrameEncoding::Delta,
        ] {
            let sequence = FrameSequence::new(2, 1_000, 40)
                .unwrap()
                .with_max_delta_frames(1)
                .unwrap();
            let mut encoder = FramesEncoder::new(sequence, surface)
                .unwrap()
                .with_profiles(raw_only_profiles())
                .unwrap()
                .with_tiles(16, 16)
                .unwrap();
            encoder.push(&first).unwrap();
            encoder.profiles = only_profile(encoding);
            let report = encoder.push(&second).unwrap();
            assert_eq!(report.storage(), FrameStorage::Sparse, "{encoding:?}");
            assert_eq!(report.encoding(), Some(encoding));
            let encoded = encoder.finish().unwrap();
            let frames = FramesView::open(encoded.payload(), &PayloadLimits::HOST).unwrap();
            let mut slots = [None];
            let mut canvas = vec![0; second.len()];
            let mut workspace = vec![0; second.len()];
            let mut playback = frames
                .test_session(
                    SurfaceRequirements::new(),
                    PayloadLimits::HOST,
                    &mut slots,
                    &mut canvas,
                    &mut workspace,
                    &mut [],
                )
                .unwrap();
            playback.present(0).unwrap();
            assert_eq!(tight_frame(playback.present(1).unwrap()), second);
        }
    }

    fn raw_only_profiles() -> FrameEncodingSet {
        FrameEncodingSet::lossless()
            .with_rle(false)
            .with_pixel(false)
            .with_lz4(false)
            .with_reversible_frequency(false)
            .with_delta(false)
    }

    fn only_profile(encoding: FrameEncoding) -> FrameEncodingSet {
        FrameEncodingSet {
            raw: false,
            rle: encoding == FrameEncoding::Rle,
            pixel: encoding == FrameEncoding::Pixel,
            lz4: encoding == FrameEncoding::Lz4,
            frequency_reversible: encoding == FrameEncoding::FrequencyReversible,
            frequency_quality: None,
            delta: encoding == FrameEncoding::Delta,
        }
    }

    fn sparse_raw_roundtrip(
        surface: SurfaceDescriptor,
        tile_width: u32,
        tile_height: u32,
        first: &[u8],
        second: &[u8],
    ) {
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        let mut encoder = FramesEncoder::new(sequence, surface)
            .unwrap()
            .with_profiles(raw_only_profiles())
            .unwrap()
            .with_tiles(tile_width, tile_height)
            .unwrap();
        if let Some(entries) = surface.sample_layout().color_table_entries() {
            let mut rgba = vec![0; entries as usize * 4];
            for color in rgba.chunks_exact_mut(4) {
                color[3] = 255;
            }
            encoder = encoder.with_color_table(&rgba).unwrap();
        }
        encoder.push(first).unwrap();
        assert_eq!(
            encoder.push(second).unwrap().storage(),
            FrameStorage::Sparse
        );
        let encoded = encoder.finish().unwrap();
        let frames = FramesView::open(encoded.payload(), &PayloadLimits::HOST).unwrap();
        let mut slots = [None];
        let mut canvas = vec![0; second.len()];
        let mut workspace = vec![0; second.len()];
        let mut playback = frames
            .test_session(
                SurfaceRequirements::new(),
                PayloadLimits::HOST,
                &mut slots,
                &mut canvas,
                &mut workspace,
                &mut [],
            )
            .unwrap();
        playback.present(0).unwrap();
        assert_eq!(tight_frame(playback.present(1).unwrap()), second);
    }

    fn tight_frame(view: crate::image::SurfaceView<'_>) -> Vec<u8> {
        view.planes()
            .flat_map(|plane| plane.rows().unwrap())
            .flatten()
            .copied()
            .collect()
    }
}
