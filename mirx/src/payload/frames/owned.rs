use alloc::{borrow::Cow, vec::Vec};
use core::{iter::FusedIterator, mem::size_of, slice};

use super::{
    FRAME_ENTRY_LEN, Frame, FrameIter, FramesDecodeError, FramesMode, FramesView, HEADER_LEN,
    MAX_FRAME_COUNT, MAX_TIMESCALE_HZ, validate_frame,
};
use crate::{
    ColorFormat,
    payload::envelope::{VERSION, checked_payload_len, write_crc_trailer},
    payload::image::{ImageAsset, ImageAssetParts},
    reader::PayloadLimits,
};

/// Failure from a positional or copy-on-write FRAMES mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FramesMutationError {
    IndexOutOfBounds { index: usize, len: usize },
    TooManyFrames { count: usize, limit: u32 },
    DecodedBytesLimitExceeded { needed: usize, limit: usize },
    AllocationFailed,
    SizeOverflow,
}

/// Failure while validating or encoding an editable FRAMES asset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FramesEncodeError {
    InvalidAsset(FramesDecodeError),
    Storage(crate::image::ImageEncodeError),
    FrameMap(super::FrameMapError),
    FrameCountMismatch {
        expected: u32,
        actual: usize,
    },
    GroupCountMismatch {
        expected: u32,
        actual: usize,
    },
    Timing(super::FrameTimingError),
    Composition(super::FrameCompositionError),
    Keyframes(super::KeyframeIndexError),
    ReferenceStorage(crate::image::EncodedImageError),
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
    MainPlaneLengthMismatch {
        expected: usize,
        actual: usize,
    },
    ExtraPlaneLengthMismatch {
        expected: usize,
        actual: usize,
    },
    DecodedBytesLimitExceeded {
        needed: usize,
        limit: usize,
    },
    BufferTooSmall {
        needed: usize,
        available: usize,
    },
    SizeOverflow,
    AllocationFailed,
}

#[derive(Debug, Eq, PartialEq)]
enum FrameTableStorage<'a> {
    Encoded { bytes: &'a [u8], count: u16 },
    Owned(Vec<Frame>),
}

#[derive(Debug, Eq, PartialEq)]
struct FramesData<'a> {
    format: ColorFormat,
    atlas_width: u32,
    atlas_height: u32,
    atlas_stride: u32,
    table: FrameTableStorage<'a>,
    main: Cow<'a, [u8]>,
    extra: Option<Cow<'a, [u8]>>,
    limits: PayloadLimits,
}

/// Editable Atlas-mode FRAMES value with partitioned copy-on-write storage.
#[derive(Debug, Eq, PartialEq)]
pub struct AtlasFrames<'a> {
    data: FramesData<'a>,
}

/// Editable Animation-mode FRAMES value with partitioned copy-on-write storage.
#[derive(Debug, Eq, PartialEq)]
pub struct AnimationFrames<'a> {
    data: FramesData<'a>,
    settings: AnimationSettings,
}

/// Playback and display settings for an Animation-mode FRAMES value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnimationSettings {
    canvas_width: u32,
    canvas_height: u32,
    timescale_hz: u32,
    default_duration_ticks: u32,
    play_count: u32,
}

/// Editable FRAMES asset in Atlas or Animation mode.
#[derive(Debug, Eq, PartialEq)]
pub enum FramesAsset<'a> {
    Atlas(AtlasFrames<'a>),
    Animation(AnimationFrames<'a>),
}

impl<'a> AtlasFrames<'a> {
    pub fn new(image: ImageAsset<'a>, frames: Vec<Frame>) -> Self {
        Self {
            data: FramesData::owned(image, frames),
        }
    }
}

impl<'a> AnimationFrames<'a> {
    pub fn new(image: ImageAsset<'a>, frames: Vec<Frame>, settings: AnimationSettings) -> Self {
        Self {
            data: FramesData::owned(image, frames),
            settings,
        }
    }

    pub const fn canvas_width(&self) -> u32 {
        self.settings.canvas_width
    }

    pub const fn canvas_height(&self) -> u32 {
        self.settings.canvas_height
    }

    pub const fn timescale_hz(&self) -> u32 {
        self.settings.timescale_hz
    }

    pub const fn default_duration_ticks(&self) -> u32 {
        self.settings.default_duration_ticks
    }

    pub const fn play_count(&self) -> u32 {
        self.settings.play_count
    }

    pub fn set_canvas_size(&mut self, width: u32, height: u32) {
        self.settings.canvas_width = width;
        self.settings.canvas_height = height;
    }

    pub fn set_timing(&mut self, timescale_hz: u32, default_duration_ticks: u32) {
        self.settings.timescale_hz = timescale_hz;
        self.settings.default_duration_ticks = default_duration_ticks;
    }

    pub fn set_play_count(&mut self, play_count: u32) {
        self.settings.play_count = play_count;
    }
}

impl AnimationSettings {
    pub const fn new(
        canvas_width: u32,
        canvas_height: u32,
        timescale_hz: u32,
        default_duration_ticks: u32,
    ) -> Self {
        Self {
            canvas_width,
            canvas_height,
            timescale_hz,
            default_duration_ticks,
            play_count: 0,
        }
    }

    pub const fn with_play_count(mut self, play_count: u32) -> Self {
        self.play_count = play_count;
        self
    }

    pub const fn canvas_width(self) -> u32 {
        self.canvas_width
    }

    pub const fn canvas_height(self) -> u32 {
        self.canvas_height
    }

    pub const fn timescale_hz(self) -> u32 {
        self.timescale_hz
    }

    pub const fn default_duration_ticks(self) -> u32 {
        self.default_duration_ticks
    }

    pub const fn play_count(self) -> u32 {
        self.play_count
    }
}

impl<'a> FramesAsset<'a> {
    /// Creates an asset that keeps the validated table and planes borrowed.
    pub fn from_view(view: FramesView<'a>, limits: PayloadLimits) -> Self {
        let data = FramesData {
            format: view.format,
            atlas_width: view.atlas_width,
            atlas_height: view.atlas_height,
            atlas_stride: view.atlas_stride,
            table: FrameTableStorage::Encoded {
                bytes: view.frame_table,
                count: view.frame_count,
            },
            main: Cow::Borrowed(view.main),
            extra: (!view.extra.is_empty()).then_some(Cow::Borrowed(view.extra)),
            limits,
        };
        match view.mode {
            FramesMode::Atlas => Self::Atlas(AtlasFrames { data }),
            FramesMode::Animation => Self::Animation(AnimationFrames {
                data,
                settings: AnimationSettings::new(
                    view.canvas_width,
                    view.canvas_height,
                    view.timescale_hz,
                    view.default_duration_ticks,
                )
                .with_play_count(view.play_count),
            }),
        }
    }

    pub const fn mode(&self) -> FramesMode {
        match self {
            Self::Atlas(_) => FramesMode::Atlas,
            Self::Animation(_) => FramesMode::Animation,
        }
    }

    pub fn len(&self) -> usize {
        self.data().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub const fn format(&self) -> ColorFormat {
        self.data().format
    }

    pub const fn atlas_width(&self) -> u32 {
        self.data().atlas_width
    }

    pub const fn atlas_height(&self) -> u32 {
        self.data().atlas_height
    }

    pub const fn atlas_stride(&self) -> u32 {
        self.data().atlas_stride
    }

    pub fn main(&self) -> &[u8] {
        self.data().main.as_ref()
    }

    pub fn extra(&self) -> Option<&[u8]> {
        self.data().extra.as_deref()
    }

    pub fn set_main(&mut self, main: Cow<'a, [u8]>) {
        self.data_mut().main = main;
    }

    pub fn set_extra(&mut self, extra: Option<Cow<'a, [u8]>>) {
        self.data_mut().extra = extra;
    }

    pub fn frames(&self) -> AssetFrameIter<'_> {
        match &self.data().table {
            FrameTableStorage::Encoded { bytes, .. } => {
                AssetFrameIter::Encoded(FrameIter { remaining: bytes })
            }
            FrameTableStorage::Owned(frames) => AssetFrameIter::Owned(frames.iter()),
        }
    }

    pub fn frame(&self, index: usize) -> Option<Frame> {
        self.frames().nth(index)
    }

    pub fn try_frames_mut(&mut self) -> Result<&mut Vec<Frame>, FramesMutationError> {
        self.data_mut().materialize_table()
    }

    /// Appends a frame after applying record and decoded-memory limits.
    pub fn push_frame(&mut self, frame: Frame) -> Result<(), FramesMutationError> {
        self.insert_frame(self.len(), frame)
    }

    pub fn insert_frame(&mut self, index: usize, frame: Frame) -> Result<(), FramesMutationError> {
        let len = self.len();
        if index > len {
            return Err(FramesMutationError::IndexOutOfBounds { index, len });
        }
        let limit = self.data().limits.max_frame_records().min(MAX_FRAME_COUNT);
        if len >= limit as usize {
            return Err(FramesMutationError::TooManyFrames {
                count: len + 1,
                limit,
            });
        }
        let table_growth = match &self.data().table {
            FrameTableStorage::Encoded { count, .. } => usize::from(*count)
                .checked_mul(size_of::<Frame>())
                .ok_or(FramesMutationError::SizeOverflow)?,
            FrameTableStorage::Owned(_) => 0,
        };
        let growth = table_growth
            .checked_add(size_of::<Frame>())
            .ok_or(FramesMutationError::SizeOverflow)?;
        self.data().check_owned_growth(growth)?;
        let frames = self.try_frames_mut()?;
        frames
            .try_reserve(1)
            .map_err(|_| FramesMutationError::AllocationFailed)?;
        frames.insert(index, frame);
        Ok(())
    }

    /// Replaces one frame and returns the previous record.
    pub fn replace_frame(
        &mut self,
        index: usize,
        frame: Frame,
    ) -> Result<Frame, FramesMutationError> {
        let len = self.len();
        if index >= len {
            return Err(FramesMutationError::IndexOutOfBounds { index, len });
        }
        Ok(core::mem::replace(
            &mut self.try_frames_mut()?[index],
            frame,
        ))
    }

    pub fn remove_frame(&mut self, index: usize) -> Result<Frame, FramesMutationError> {
        let len = self.len();
        if index >= len {
            return Err(FramesMutationError::IndexOutOfBounds { index, len });
        }
        Ok(self.try_frames_mut()?.remove(index))
    }

    pub fn move_frame(&mut self, from: usize, to: usize) -> Result<(), FramesMutationError> {
        let len = self.len();
        if from >= len {
            return Err(FramesMutationError::IndexOutOfBounds { index: from, len });
        }
        if to >= len {
            return Err(FramesMutationError::IndexOutOfBounds { index: to, len });
        }
        let frames = self.try_frames_mut()?;
        if from < to {
            frames[from..=to].rotate_left(1);
        } else if to < from {
            frames[to..=from].rotate_right(1);
        }
        Ok(())
    }

    pub fn try_main_mut(&mut self) -> Result<&mut [u8], FramesMutationError> {
        self.data_mut().materialize_main()
    }

    pub fn try_extra_mut(&mut self) -> Result<Option<&mut [u8]>, FramesMutationError> {
        self.data_mut().materialize_extra()
    }

    pub fn encoded_payload_len(&self) -> Result<usize, FramesEncodeError> {
        self.encoded_payload_len_with_limits(self.data().limits)
    }

    pub fn encoded_payload_len_with_limits(
        &self,
        limits: PayloadLimits,
    ) -> Result<usize, FramesEncodeError> {
        Ok(FramesPayloadPlan::new(self, limits)?.encoded_len)
    }

    pub fn encode_payload_into(&self, out: &mut [u8]) -> Result<usize, FramesEncodeError> {
        self.encode_payload_into_with_limits(out, self.data().limits)
    }

    pub fn encode_payload_into_with_limits(
        &self,
        out: &mut [u8],
        limits: PayloadLimits,
    ) -> Result<usize, FramesEncodeError> {
        FramesPayloadPlan::new(self, limits)?.copy_into(out)
    }

    pub fn encode_payload(&self) -> Result<Vec<u8>, FramesEncodeError> {
        self.encode_payload_with_limits(self.data().limits)
    }

    pub fn encode_payload_with_limits(
        &self,
        limits: PayloadLimits,
    ) -> Result<Vec<u8>, FramesEncodeError> {
        let plan = FramesPayloadPlan::new(self, limits)?;
        let mut out = Vec::new();
        out.try_reserve_exact(plan.encoded_len)
            .map_err(|_| FramesEncodeError::AllocationFailed)?;
        out.resize(plan.encoded_len, 0);
        plan.emit(&mut out);
        Ok(out)
    }

    /// Returns whether `candidate` is this asset's exact canonical payload.
    pub fn matches_payload(&self, candidate: &[u8]) -> bool {
        self.matches_payload_with_limits(candidate, self.data().limits)
    }

    pub(crate) fn matches_payload_with_limits(
        &self,
        candidate: &[u8],
        limits: PayloadLimits,
    ) -> bool {
        let Ok(plan) = FramesPayloadPlan::new(self, limits) else {
            return false;
        };
        let Ok(view) = FramesView::open_payload(candidate, &limits) else {
            return false;
        };
        candidate.len() == plan.encoded_len
            && view.mode == self.mode()
            && view.format == self.format()
            && view.atlas_width == self.atlas_width()
            && view.atlas_height == self.atlas_height()
            && view.atlas_stride == self.atlas_stride()
            && view.frames().eq(self.frames())
            && view.main == self.main()
            && view.extra == self.extra().unwrap_or(&[])
            && plan.mode_fields_match(view)
    }

    const fn data(&self) -> &FramesData<'a> {
        match self {
            Self::Atlas(value) => &value.data,
            Self::Animation(value) => &value.data,
        }
    }

    fn data_mut(&mut self) -> &mut FramesData<'a> {
        match self {
            Self::Atlas(value) => &mut value.data,
            Self::Animation(value) => &mut value.data,
        }
    }
}

impl<'a> FramesData<'a> {
    fn owned(image: ImageAsset<'a>, frames: Vec<Frame>) -> Self {
        let ImageAssetParts { meta, main, extra } = image.into_parts();
        Self {
            format: meta.format,
            atlas_width: meta.width,
            atlas_height: meta.height,
            atlas_stride: meta.stride,
            table: FrameTableStorage::Owned(frames),
            main,
            extra,
            limits: PayloadLimits::HOST,
        }
    }

    fn len(&self) -> usize {
        match &self.table {
            FrameTableStorage::Encoded { count, .. } => usize::from(*count),
            FrameTableStorage::Owned(frames) => frames.len(),
        }
    }

    fn owned_bytes(&self) -> Result<usize, FramesMutationError> {
        let table = match &self.table {
            FrameTableStorage::Encoded { .. } => 0,
            FrameTableStorage::Owned(frames) => frames
                .len()
                .checked_mul(size_of::<Frame>())
                .ok_or(FramesMutationError::SizeOverflow)?,
        };
        let main = matches!(self.main, Cow::Owned(_))
            .then_some(self.main.len())
            .unwrap_or(0);
        let extra = self
            .extra
            .as_ref()
            .filter(|extra| matches!(extra, Cow::Owned(_)))
            .map_or(0, |extra| extra.len());
        table
            .checked_add(main)
            .and_then(|bytes| bytes.checked_add(extra))
            .ok_or(FramesMutationError::SizeOverflow)
    }

    fn check_owned_growth(&self, growth: usize) -> Result<(), FramesMutationError> {
        let needed = self
            .owned_bytes()?
            .checked_add(growth)
            .ok_or(FramesMutationError::SizeOverflow)?;
        let limit = self.limits.max_decoded_bytes();
        if needed > limit {
            return Err(FramesMutationError::DecodedBytesLimitExceeded { needed, limit });
        }
        Ok(())
    }

    fn materialize_table(&mut self) -> Result<&mut Vec<Frame>, FramesMutationError> {
        if let FrameTableStorage::Encoded { bytes, count } = &self.table {
            let decoded_bytes = usize::from(*count)
                .checked_mul(size_of::<Frame>())
                .ok_or(FramesMutationError::SizeOverflow)?;
            self.check_owned_growth(decoded_bytes)?;
            let mut frames = Vec::new();
            frames
                .try_reserve_exact(usize::from(*count))
                .map_err(|_| FramesMutationError::AllocationFailed)?;
            frames.extend(FrameIter { remaining: bytes });
            self.table = FrameTableStorage::Owned(frames);
        }
        match &mut self.table {
            FrameTableStorage::Owned(frames) => Ok(frames),
            FrameTableStorage::Encoded { .. } => unreachable!(),
        }
    }

    fn materialize_main(&mut self) -> Result<&mut [u8], FramesMutationError> {
        if let Cow::Borrowed(bytes) = self.main {
            self.check_owned_growth(bytes.len())?;
            self.main = Cow::Owned(copy_bytes(bytes)?);
        }
        match &mut self.main {
            Cow::Owned(bytes) => Ok(bytes),
            Cow::Borrowed(_) => unreachable!(),
        }
    }

    fn materialize_extra(&mut self) -> Result<Option<&mut [u8]>, FramesMutationError> {
        let borrowed_len = self.extra.as_ref().and_then(|extra| match extra {
            Cow::Borrowed(bytes) => Some(bytes.len()),
            Cow::Owned(_) => None,
        });
        if let Some(len) = borrowed_len {
            self.check_owned_growth(len)?;
            let source = self.extra.as_deref().unwrap();
            self.extra = Some(Cow::Owned(copy_bytes(source)?));
        }
        Ok(match self.extra.as_mut() {
            Some(Cow::Owned(bytes)) => Some(bytes.as_mut_slice()),
            Some(Cow::Borrowed(_)) => unreachable!(),
            None => None,
        })
    }
}

fn copy_bytes(bytes: &[u8]) -> Result<Vec<u8>, FramesMutationError> {
    let mut owned = Vec::new();
    owned
        .try_reserve_exact(bytes.len())
        .map_err(|_| FramesMutationError::AllocationFailed)?;
    owned.extend_from_slice(bytes);
    Ok(owned)
}

/// Iterator over borrowed encoded or materialized frame records.
#[derive(Clone, Debug)]
pub enum AssetFrameIter<'a> {
    Encoded(FrameIter<'a>),
    Owned(slice::Iter<'a, Frame>),
}

impl Iterator for AssetFrameIter<'_> {
    type Item = Frame;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Encoded(iter) => iter.next(),
            Self::Owned(iter) => iter.next().copied(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.len();
        (len, Some(len))
    }
}

impl DoubleEndedIterator for AssetFrameIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        match self {
            Self::Encoded(iter) => iter.next_back(),
            Self::Owned(iter) => iter.next_back().copied(),
        }
    }
}

impl ExactSizeIterator for AssetFrameIter<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Encoded(iter) => iter.len(),
            Self::Owned(iter) => iter.len(),
        }
    }
}

impl FusedIterator for AssetFrameIter<'_> {}

struct FramesPayloadPlan<'a> {
    asset: &'a FramesAsset<'a>,
    frame_count: u16,
    data_offset: u32,
    main_size: u32,
    extra_size: u32,
    stored_size: u32,
    covered_len: usize,
    encoded_len: usize,
}

impl<'a> FramesPayloadPlan<'a> {
    fn new(asset: &'a FramesAsset<'a>, limits: PayloadLimits) -> Result<Self, FramesEncodeError> {
        let data = asset.data();
        if data.atlas_width == 0 {
            return Err(invalid(FramesDecodeError::ZeroAtlasWidth));
        }
        if data.atlas_height == 0 {
            return Err(invalid(FramesDecodeError::ZeroAtlasHeight));
        }
        let minimum = data
            .format
            .minimum_stride(data.atlas_width)
            .ok_or_else(|| invalid(FramesDecodeError::SizeOverflow))?;
        if data.atlas_stride < minimum {
            return Err(invalid(FramesDecodeError::StrideTooSmall {
                minimum,
                actual: data.atlas_stride,
            }));
        }
        let count = data.len();
        if count == 0 || count > MAX_FRAME_COUNT as usize {
            return Err(invalid(FramesDecodeError::InvalidFrameCount(
                u32::try_from(count).unwrap_or(u32::MAX),
            )));
        }
        let count_u32 = count as u32;
        let limit = limits.max_frame_records();
        if count_u32 > limit {
            return Err(invalid(FramesDecodeError::TooManyFrames {
                count: count_u32,
                limit,
            }));
        }

        let (canvas_width, canvas_height) = match asset {
            FramesAsset::Atlas(_) => (0, 0),
            FramesAsset::Animation(animation) => {
                if animation.canvas_width() == 0 {
                    return Err(invalid(FramesDecodeError::ZeroCanvasWidth));
                }
                if animation.canvas_height() == 0 {
                    return Err(invalid(FramesDecodeError::ZeroCanvasHeight));
                }
                if !(1..=MAX_TIMESCALE_HZ).contains(&animation.timescale_hz()) {
                    return Err(invalid(FramesDecodeError::InvalidTimescale(
                        animation.timescale_hz(),
                    )));
                }
                if animation.default_duration_ticks() == 0 {
                    return Err(invalid(FramesDecodeError::ZeroDefaultDuration));
                }
                (animation.canvas_width(), animation.canvas_height())
            }
        };
        for (index, frame) in asset.frames().enumerate() {
            validate_frame(
                frame,
                index as u16,
                asset.mode(),
                data.atlas_width,
                data.atlas_height,
                canvas_width,
                canvas_height,
            )
            .map_err(invalid)?;
        }

        let main_size = data
            .atlas_stride
            .checked_mul(data.atlas_height)
            .ok_or_else(|| invalid(FramesDecodeError::SizeOverflow))?;
        let expected_main = main_size as usize;
        if data.main.len() != expected_main {
            return Err(FramesEncodeError::MainPlaneLengthMismatch {
                expected: expected_main,
                actual: data.main.len(),
            });
        }
        let extra_size = data
            .format
            .extra_size(data.atlas_width, data.atlas_height, data.atlas_stride)
            .ok_or_else(|| invalid(FramesDecodeError::SizeOverflow))?;
        let expected_extra = extra_size as usize;
        let actual_extra = data.extra.as_deref().map_or(0, <[u8]>::len);
        if actual_extra != expected_extra {
            return Err(FramesEncodeError::ExtraPlaneLengthMismatch {
                expected: expected_extra,
                actual: actual_extra,
            });
        }
        let owned = data.owned_bytes().map_err(map_mutation_error)?;
        let decoded_limit = limits.max_decoded_bytes();
        if owned > decoded_limit {
            return Err(FramesEncodeError::DecodedBytesLimitExceeded {
                needed: owned,
                limit: decoded_limit,
            });
        }
        let table_size = count_u32
            .checked_mul(FRAME_ENTRY_LEN as u32)
            .ok_or_else(|| invalid(FramesDecodeError::SizeOverflow))?;
        let data_offset = (HEADER_LEN as u32)
            .checked_add(table_size)
            .ok_or_else(|| invalid(FramesDecodeError::SizeOverflow))?;
        let stored_size = main_size
            .checked_add(extra_size)
            .ok_or_else(|| invalid(FramesDecodeError::SizeOverflow))?;
        let covered_u32 = data_offset
            .checked_add(stored_size)
            .ok_or_else(|| invalid(FramesDecodeError::SizeOverflow))?;
        let covered_len = covered_u32 as usize;
        let encoded_len = checked_payload_len(covered_len)
            .map_err(|_| invalid(FramesDecodeError::SizeOverflow))?;

        Ok(Self {
            asset,
            frame_count: count as u16,
            data_offset,
            main_size,
            extra_size,
            stored_size,
            covered_len,
            encoded_len,
        })
    }

    fn copy_into(self, out: &mut [u8]) -> Result<usize, FramesEncodeError> {
        if out.len() < self.encoded_len {
            return Err(FramesEncodeError::BufferTooSmall {
                needed: self.encoded_len,
                available: out.len(),
            });
        }
        self.emit(&mut out[..self.encoded_len]);
        Ok(self.encoded_len)
    }

    fn emit(&self, out: &mut [u8]) {
        out.fill(0);
        let data = self.asset.data();
        out[0] = VERSION;
        out[1] = self.asset.mode() as u8;
        out[2] = data.format.to_u8();
        write_u32(out, 8, data.atlas_width);
        write_u32(out, 12, data.atlas_height);
        write_u32(out, 16, data.atlas_stride);
        if let FramesAsset::Animation(animation) = self.asset {
            write_u32(out, 20, animation.canvas_width());
            write_u32(out, 24, animation.canvas_height());
            write_u32(out, 32, animation.timescale_hz());
            write_u32(out, 36, animation.default_duration_ticks());
            write_u32(out, 40, animation.play_count());
        }
        write_u32(out, 28, u32::from(self.frame_count));
        write_u32(out, 44, HEADER_LEN as u32);
        write_u32(out, 48, self.data_offset);
        write_u32(out, 52, self.stored_size);
        write_u32(out, 56, self.main_size);
        write_u32(out, 60, self.extra_size);
        for (index, frame) in self.asset.frames().enumerate() {
            let start = HEADER_LEN + index * FRAME_ENTRY_LEN;
            write_frame(&mut out[start..start + FRAME_ENTRY_LEN], frame);
        }
        let data_start = self.data_offset as usize;
        let main_end = data_start + self.main_size as usize;
        out[data_start..main_end].copy_from_slice(self.asset.main());
        if let Some(extra) = self.asset.extra() {
            out[main_end..self.covered_len].copy_from_slice(extra);
        }
        let (covered, trailer) = out.split_at_mut(self.covered_len);
        let mut crc = [0; 4];
        write_crc_trailer(covered, &mut crc);
        trailer.copy_from_slice(&crc);
    }

    fn mode_fields_match(&self, view: FramesView<'_>) -> bool {
        match self.asset {
            FramesAsset::Atlas(_) => true,
            FramesAsset::Animation(animation) => {
                view.canvas_width == animation.canvas_width()
                    && view.canvas_height == animation.canvas_height()
                    && view.timescale_hz == animation.timescale_hz()
                    && view.default_duration_ticks == animation.default_duration_ticks()
                    && view.play_count == animation.play_count()
            }
        }
    }
}

fn invalid(error: FramesDecodeError) -> FramesEncodeError {
    FramesEncodeError::InvalidAsset(error)
}

fn map_mutation_error(error: FramesMutationError) -> FramesEncodeError {
    match error {
        FramesMutationError::DecodedBytesLimitExceeded { needed, limit } => {
            FramesEncodeError::DecodedBytesLimitExceeded { needed, limit }
        }
        _ => invalid(FramesDecodeError::SizeOverflow),
    }
}

fn write_u32(out: &mut [u8], offset: usize, value: u32) {
    out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_frame(out: &mut [u8], frame: Frame) {
    write_u32(out, 0, frame.source_x);
    write_u32(out, 4, frame.source_y);
    write_u32(out, 8, frame.width);
    write_u32(out, 12, frame.height);
    write_u32(out, 16, frame.target_x);
    write_u32(out, 20, frame.target_y);
    write_u32(out, 24, frame.duration_ticks);
}

#[cfg(test)]
mod tests {
    use alloc::{borrow::Cow, vec, vec::Vec};

    use super::*;

    fn frame(x: u32, duration_ticks: u32) -> Frame {
        Frame {
            source_x: x,
            source_y: 0,
            width: 1,
            height: 1,
            target_x: 0,
            target_y: 0,
            duration_ticks,
        }
    }

    fn atlas(format: ColorFormat) -> FramesAsset<'static> {
        let stride = format.minimum_stride(3).unwrap();
        let extra_len = format.extra_size(3, 1, stride).unwrap() as usize;
        let image = ImageAsset::new(
            3,
            1,
            format,
            stride,
            Cow::Owned(vec![0x55; stride as usize]),
        );
        let image = if extra_len == 0 {
            image
        } else {
            image.with_extra(Cow::Owned(vec![0x80; extra_len]))
        };
        FramesAsset::Atlas(AtlasFrames::new(
            image,
            vec![frame(0, 0), frame(1, 0), frame(2, 0)],
        ))
    }

    fn animation() -> FramesAsset<'static> {
        FramesAsset::Animation(AnimationFrames::new(
            ImageAsset::new(3, 1, ColorFormat::A8, 3, Cow::Owned(vec![1, 2, 3])),
            vec![frame(0, 0), frame(1, 2), frame(2, 3)],
            AnimationSettings::new(4, 1, 60, 1),
        ))
    }

    #[test]
    fn canonical_encoder_round_trips_every_color_format() {
        let formats = [
            ColorFormat::I1,
            ColorFormat::I2,
            ColorFormat::I4,
            ColorFormat::I8,
            ColorFormat::A1,
            ColorFormat::A2,
            ColorFormat::A4,
            ColorFormat::A8,
            ColorFormat::L8,
            ColorFormat::RGB565,
            ColorFormat::RGB565Swapped,
            ColorFormat::RGB565A8,
            ColorFormat::RGB888,
            ColorFormat::XRGB8888,
            ColorFormat::RGBA8888,
            ColorFormat::BGRA8888,
        ];
        for format in formats {
            let asset = atlas(format);
            let bytes = asset.encode_payload().unwrap();
            assert_eq!(bytes.len(), asset.encoded_payload_len().unwrap());
            assert!(asset.matches_payload(&bytes));
            let view = FramesView::open_payload(&bytes, &PayloadLimits::HOST).unwrap();
            assert_eq!(view.format(), format);
            assert_eq!(
                view.frames().collect::<Vec<_>>(),
                asset.frames().collect::<Vec<_>>()
            );
            assert_eq!(view.atlas().main(), asset.main());
            assert_eq!(view.atlas().extra(), asset.extra());
        }
    }

    #[test]
    fn animation_fields_and_crc_round_trip() {
        let asset = animation();
        let bytes = asset.encode_payload().unwrap();
        let view = FramesView::open_payload(&bytes, &PayloadLimits::HOST).unwrap();
        assert_eq!(view.mode(), FramesMode::Animation);
        assert_eq!(view.canvas_width(), 4);
        assert_eq!(view.canvas_height(), 1);
        assert_eq!(view.timescale_hz(), 60);
        assert_eq!(view.default_duration_ticks(), 1);
        assert_eq!(view.play_count(), 0);
        assert_eq!(view.frame(1).unwrap().duration_ticks, 2);
    }

    #[test]
    fn borrowed_parts_materialize_independently() {
        let bytes = animation().encode_payload().unwrap();
        let view = FramesView::open_payload(&bytes, &PayloadLimits::HOST).unwrap();
        let table_ptr = view.frame_table.as_ptr();
        let main_ptr = view.main.as_ptr();
        let mut asset = FramesAsset::from_view(view, PayloadLimits::HOST);

        if let FramesAsset::Animation(animation) = &mut asset {
            animation.set_timing(120, 2);
            animation.set_play_count(4);
        } else {
            panic!("expected animation");
        }
        assert_eq!(asset.main().as_ptr(), main_ptr);
        assert_eq!(asset.frames().next(), Some(frame(0, 0)));
        assert_eq!(asset.frames().next().unwrap().source_x, 0);
        assert_eq!(asset.try_frames_mut().unwrap().len(), 3);
        match asset.frames() {
            AssetFrameIter::Owned(iter) => {
                assert_ne!(iter.as_slice().as_ptr().cast::<u8>(), table_ptr);
            }
            AssetFrameIter::Encoded(_) => panic!("table stayed encoded"),
        }
        assert_eq!(asset.main().as_ptr(), main_ptr);
        asset.try_main_mut().unwrap()[0] = 9;
        assert_ne!(asset.main().as_ptr(), main_ptr);
        assert_eq!(asset.main()[0], 9);
    }

    #[test]
    fn positional_frame_crud_preserves_order() {
        let mut asset = atlas(ColorFormat::A8);
        let replacement = frame(1, 0);
        assert_eq!(asset.remove_frame(1).unwrap().source_x, 1);
        asset.push_frame(replacement).unwrap();
        asset.move_frame(2, 1).unwrap();
        asset.move_frame(0, 2).unwrap();
        assert_eq!(
            asset
                .frames()
                .map(|frame| frame.source_x)
                .collect::<Vec<_>>(),
            vec![1, 2, 0]
        );
        assert_eq!(asset.replace_frame(2, frame(0, 0)).unwrap().source_x, 0);
        assert!(asset.encode_payload().is_ok());
    }

    #[test]
    fn capacity_and_validation_failures_leave_output_unchanged() {
        let asset = animation();
        let needed = asset.encoded_payload_len().unwrap();
        let mut short = vec![0xa5; needed - 1];
        assert_eq!(
            asset.encode_payload_into(&mut short),
            Err(FramesEncodeError::BufferTooSmall {
                needed,
                available: needed - 1,
            })
        );
        assert!(short.iter().all(|byte| *byte == 0xa5));

        let mut invalid = atlas(ColorFormat::A8);
        invalid.set_main(Cow::Owned(vec![0; 2]));
        assert_eq!(
            invalid.encode_payload(),
            Err(FramesEncodeError::MainPlaneLengthMismatch {
                expected: 3,
                actual: 2,
            })
        );
    }

    #[test]
    fn decoded_budget_blocks_each_first_materialization() {
        let bytes = animation().encode_payload().unwrap();
        let view = FramesView::open_payload(&bytes, &PayloadLimits::HOST).unwrap();
        let mut asset = FramesAsset::from_view(view, PayloadLimits::HOST.with_max_decoded_bytes(0));
        assert!(matches!(
            asset.try_frames_mut(),
            Err(FramesMutationError::DecodedBytesLimitExceeded { .. })
        ));
        assert!(matches!(
            asset.try_main_mut(),
            Err(FramesMutationError::DecodedBytesLimitExceeded { .. })
        ));
        assert!(asset.matches_payload(&bytes));
    }
}
