use alloc::vec::Vec;

use super::{
    FrameCandidate, FrameChoice, FramePolicy, FrameSelectionError, FrameSelector, FrameSequence,
    FrameStorage, FramesEncodeError, SectionedFramesAsset,
};
use crate::{
    coding::{FrameDelta, FrameDeltaError, Lz4, Lz4Error, Pixel, PixelError, Rle, RleError},
    image::{ReferenceMode, SurfaceDescriptor, UNIT_GROUP_RECORD_LEN, UnitGroupRecord},
    media::{CODING_RECORD_LEN, CodingId, CodingRecord},
};

/// Lossless coding profiles considered by [`FramesEncoder`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameEncodingSet {
    raw: bool,
    rle: bool,
    pixel: bool,
    lz4: bool,
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
    Delta,
}

impl FrameEncoding {
    pub const fn coding_id(self) -> CodingId {
        match self {
            Self::Raw => CodingId::RAW,
            Self::Rle => CodingId::RLE,
            Self::Pixel => CodingId::PIXEL,
            Self::Lz4 => CodingId::LZ4,
            Self::Delta => CodingId::FRAME_DELTA,
        }
    }

    const fn record(self) -> CodingRecord<'static> {
        match self {
            Self::Raw => CodingRecord::RAW,
            Self::Rle => Rle::new().record(),
            Self::Pixel => CodingRecord::new(CodingId::PIXEL, 1, &[]),
            Self::Lz4 => Lz4::new().record(),
            Self::Delta => FrameDelta::new().record(),
        }
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

#[derive(Debug)]
struct PreparedCandidate {
    candidate: FrameCandidate,
    encoding: Option<FrameEncoding>,
    bytes: Vec<u8>,
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
    input_alignment: u32,
    sample_bytes: usize,
    previous: Vec<u8>,
    codings: Vec<FrameEncoding>,
    groups: Vec<UnitGroupRecord>,
    frame_group_counts: Vec<u32>,
    keyframes: Vec<u32>,
    data: Vec<u8>,
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
            .try_reserve_exact(8)
            .map_err(|_| FrameWriteError::AllocationFailed)?;
        Ok(Self {
            sequence,
            surface,
            profiles: FrameEncodingSet::default(),
            selector: FrameSelector::new(FramePolicy::new(sequence.max_delta_frames())),
            input_alignment: 1,
            sample_bytes,
            previous,
            codings: Vec::new(),
            groups: Vec::new(),
            frame_group_counts: Vec::new(),
            keyframes: Vec::new(),
            data: Vec::new(),
            reports: Vec::new(),
            candidates,
            lz4_table,
        })
    }

    pub fn with_profiles(mut self, profiles: FrameEncodingSet) -> Self {
        self.profiles = profiles;
        self
    }

    pub fn with_policy(mut self, policy: FramePolicy) -> Result<Self, FrameWriteError> {
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
    pub fn with_input_alignment(mut self, alignment: u32) -> Result<Self, FrameWriteError> {
        if !alignment.is_power_of_two() {
            return Err(FrameWriteError::InvalidInputAlignment(alignment));
        }
        self.input_alignment = alignment;
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

    /// Selects and stores one frame atomically.
    pub fn push(&mut self, samples: &[u8]) -> Result<FrameWriteReport, FrameWriteError> {
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

        self.prepare_candidates(samples)?;
        let mut offered = [FrameCandidate::omitted(); 8];
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
        let report = self.commit(choice, prepared, samples)?;
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
        let records: Vec<_> = self
            .codings
            .iter()
            .copied()
            .map(FrameEncoding::record)
            .collect();
        let asset = SectionedFramesAsset::new(
            self.sequence,
            self.surface,
            &records,
            &self.groups,
            &self.frame_group_counts,
            &self.data,
        )?
        .with_keyframes(&self.keyframes)?;
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
            });
        }
        if self.profiles.raw {
            let mut bytes = allocated(samples.len())?;
            bytes.copy_from_slice(samples);
            self.add_candidate(FrameStorage::Keyframe, FrameEncoding::Raw, bytes)?;
        }
        if self.profiles.rle {
            let codec = Rle::new();
            let mut bytes = allocated(codec.encoded_len(samples)?)?;
            let len = codec.encode_into(samples, &mut bytes)?;
            bytes.truncate(len);
            self.add_candidate(FrameStorage::Keyframe, FrameEncoding::Rle, bytes)?;
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
            self.add_candidate(FrameStorage::Keyframe, FrameEncoding::Pixel, bytes)?;
        }
        if self.profiles.lz4 {
            let mut encoder = Lz4::new().encoder(&mut self.lz4_table)?;
            let mut bytes = allocated(encoder.encoded_len(samples)?)?;
            let len = encoder.encode_into(samples, &mut bytes)?;
            bytes.truncate(len);
            self.add_candidate(FrameStorage::Keyframe, FrameEncoding::Lz4, bytes)?;
        }
        if self.profiles.delta && !self.previous.is_empty() {
            let codec = FrameDelta::new();
            let mut bytes = allocated(codec.encoded_len(&self.previous, samples)?)?;
            let len = codec.encode_into(&self.previous, samples, &mut bytes)?;
            bytes.truncate(len);
            self.add_candidate(FrameStorage::Delta, FrameEncoding::Delta, bytes)?;
        }
        Ok(())
    }

    fn add_candidate(
        &mut self,
        storage: FrameStorage,
        encoding: FrameEncoding,
        bytes: Vec<u8>,
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
        let candidate = match storage {
            FrameStorage::Keyframe => FrameCandidate::keyframe(encoding.coding_id(), stored),
            FrameStorage::Delta => FrameCandidate::delta(stored),
            FrameStorage::Sparse => FrameCandidate::sparse(encoding.coding_id(), stored),
            FrameStorage::Omitted => FrameCandidate::omitted(),
        }
        .with_decode_cost(self.sample_bytes as u64, self.sample_bytes as u64);
        self.candidates.push(PreparedCandidate {
            candidate,
            encoding: Some(encoding),
            bytes,
        });
        Ok(())
    }

    fn commit(
        &mut self,
        choice: FrameChoice,
        prepared: PreparedCandidate,
        samples: &[u8],
    ) -> Result<FrameWriteReport, FrameWriteError> {
        let encoded_bytes =
            u32::try_from(prepared.bytes.len()).map_err(|_| FrameWriteError::SizeOverflow)?;
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
            if choice.candidate().storage() == FrameStorage::Delta {
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
            self.groups
                .try_reserve(1)
                .map_err(|_| FrameWriteError::AllocationFailed)?;
            self.frame_group_counts
                .try_reserve(1)
                .map_err(|_| FrameWriteError::AllocationFailed)?;
            self.reports
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
            self.groups.push(group);
            self.frame_group_counts.push(1);
            if choice.candidate().storage().is_independent() {
                self.keyframes.push(choice.frame());
            }
        } else {
            self.frame_group_counts
                .try_reserve(1)
                .map_err(|_| FrameWriteError::AllocationFailed)?;
            self.reports
                .try_reserve(1)
                .map_err(|_| FrameWriteError::AllocationFailed)?;
            self.frame_group_counts.push(0);
        }
        self.previous.clear();
        self.previous.extend_from_slice(samples);
        let report = FrameWriteReport {
            frame: choice.frame(),
            storage: choice.candidate().storage(),
            encoding: prepared.encoding,
            encoded_bytes,
            stored_bytes: choice.candidate().stored_bytes(),
            delta_frames: choice.delta_frames(),
        };
        self.reports.push(report);
        Ok(report)
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

fn aligned_padding(offset: usize, alignment: u32) -> Result<usize, FrameWriteError> {
    let alignment = usize::try_from(alignment).map_err(|_| FrameWriteError::SizeOverflow)?;
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

/// Failure while selecting, storing, or finishing a sectioned frame sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FrameWriteError {
    AllocationFailed,
    SizeOverflow,
    InvalidInputAlignment(u32),
    DeltaPolicyExceedsSequence { policy: u16, sequence: u16 },
    SampleLengthMismatch { expected: usize, actual: usize },
    TooManyFrames { expected: u32 },
    FrameCountMismatch { expected: u32, actual: u32 },
    Selection(FrameSelectionError),
    Rle(RleError),
    Pixel(PixelError),
    Lz4(Lz4Error),
    Delta(FrameDeltaError),
    Group(crate::image::UnitGroupRecordError),
    Payload(FramesEncodeError),
}

impl From<FrameSelectionError> for FrameWriteError {
    fn from(error: FrameSelectionError) -> Self {
        Self::Selection(error)
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

impl From<crate::image::UnitGroupRecordError> for FrameWriteError {
    fn from(error: crate::image::UnitGroupRecordError) -> Self {
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
        image::{ColorDescription, SampleLayout, SurfaceRequirements},
        payload::frames::SectionedFramesView,
    };

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
            .with_lz4(false);
        let mut encoder = FramesEncoder::new(sequence, surface())
            .unwrap()
            .with_profiles(profiles);
        for frame in [&first[..], &same, &changed, &last] {
            encoder.push(frame).unwrap();
        }
        let encoded = encoder.finish().unwrap();
        assert_eq!(encoded.reports()[0].storage(), FrameStorage::Keyframe);
        assert_eq!(encoded.reports()[1].storage(), FrameStorage::Omitted);
        assert_eq!(encoded.reports()[2].storage(), FrameStorage::Delta);
        assert_eq!(encoded.reports()[3].storage(), FrameStorage::Keyframe);

        let frames = SectionedFramesView::open(encoded.payload(), &PayloadLimits::HOST).unwrap();
        let mut groups = [None];
        let mut canvas = [0; 16];
        let mut workspace = [0; 16];
        let mut playback = frames
            .session(
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
    fn aligns_group_starts_and_reports_complete_incremental_bytes() {
        let sequence = FrameSequence::new(2, 1_000, 40)
            .unwrap()
            .with_max_delta_frames(1)
            .unwrap();
        let profiles = FrameEncodingSet::lossless()
            .with_rle(false)
            .with_pixel(false)
            .with_lz4(false)
            .with_delta(false);
        let mut encoder = FramesEncoder::new(sequence, surface())
            .unwrap()
            .with_profiles(profiles)
            .with_input_alignment(64)
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
        let frames = SectionedFramesView::open_at(
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
}
