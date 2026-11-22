//! Allocation-free MIRX frame playback over caller-owned storage.

use super::texture::{MirxLoadError, MirxTextureOptions, Texture, TextureMeta, map_mirx_format};
use mirx::frames::{
    FrameDecodeError, FramePosition, FrameSession, FrameTimeline, FramesError, FramesPlaybackPlan,
    PlaybackStorage,
};

/// Failure while opening, planning, or presenting a MIRX frame sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MirxFramesError {
    Read(mirx::reader::ReadError),
    Frames(FramesError),
    Playback(FrameDecodeError),
    UnsupportedFormat(mirx::image::ColorFormat),
    UnsupportedLayout(mirx::image::SampleLayout),
    NoFramesChunk,
    DimensionOverflow,
}

impl From<mirx::reader::ReadError> for MirxFramesError {
    fn from(error: mirx::reader::ReadError) -> Self {
        Self::Read(error)
    }
}

impl From<FramesError> for MirxFramesError {
    fn from(error: FramesError) -> Self {
        Self::Frames(error)
    }
}

impl From<FrameDecodeError> for MirxFramesError {
    fn from(error: FrameDecodeError) -> Self {
        Self::Playback(error)
    }
}

impl From<MirxLoadError> for MirxFramesError {
    fn from(error: MirxLoadError) -> Self {
        match error {
            MirxLoadError::UnsupportedFormat(format) => Self::UnsupportedFormat(format),
            MirxLoadError::UnsupportedLayout(layout) => Self::UnsupportedLayout(layout),
            MirxLoadError::DimensionOverflow => Self::DimensionOverflow,
            _ => unreachable!("surface conversion only returns render-layout errors"),
        }
    }
}

/// Preflighted MIRX frame sequence with exact reusable storage requirements.
#[derive(Clone, Copy, Debug)]
pub struct MirxFramesPlan<'source> {
    inner: FramesPlaybackPlan<'source>,
    meta: TextureMeta,
}

/// Caller-owned storage retained by a MIRX frame playback session.
pub struct MirxFramesStorage<'source, 'storage> {
    pub groups: &'storage mut [Option<mirx::image::UnitGroup<'source>>],
    pub canvas: &'storage mut [u8],
    pub workspace: &'storage mut [u8],
    pub backup: &'storage mut [u8],
}

impl<'source> MirxFramesPlan<'source> {
    /// Opens the primary FRAMES chunk and validates every frame representation.
    ///
    /// Planning uses caller-owned group slots temporarily. The same storage may
    /// be reused when binding the playback session.
    pub fn open(
        bytes: &'source [u8],
        options: MirxTextureOptions,
        group_slots: &mut [Option<mirx::image::UnitGroup<'source>>],
    ) -> Result<Self, MirxFramesError> {
        let request = options.decode_request();
        request
            .validate_reconstruction()
            .map_err(FrameDecodeError::Request)?;
        let reader = mirx::Reader::open(bytes)?;
        let primary = reader.primary()?.ok_or(MirxFramesError::NoFramesChunk)?;
        let frames = primary
            .frames(&options.limits())?
            .ok_or(MirxFramesError::NoFramesChunk)?;
        let surface = frames.surface();
        if surface.plane_count() != 1 {
            return Err(MirxFramesError::UnsupportedLayout(surface.sample_layout()));
        }
        let source_format = surface
            .sample_layout()
            .color_format()
            .ok_or(MirxFramesError::UnsupportedLayout(surface.sample_layout()))?;
        let format = map_mirx_format(source_format)?;
        let meta = TextureMeta {
            width: surface
                .width()
                .try_into()
                .map_err(|_| MirxFramesError::DimensionOverflow)?,
            height: surface
                .height()
                .try_into()
                .map_err(|_| MirxFramesError::DimensionOverflow)?,
            format,
        };
        let inner = frames.playback_plan_for(request, options.limits(), group_slots)?;
        Ok(Self { inner, meta })
    }

    pub const fn meta(self) -> TextureMeta {
        self.meta
    }

    pub const fn frame_count(self) -> u32 {
        self.inner.frames().sequence().frame_count()
    }

    pub const fn timescale_hz(self) -> u32 {
        self.inner.frames().sequence().timescale_hz()
    }

    pub const fn play_count(self) -> u32 {
        self.inner.frames().sequence().play_count()
    }

    pub fn duration_ticks(self, frame: u32) -> Option<u32> {
        self.inner
            .frames()
            .frame(frame)
            .map(|presentation| presentation.duration_ticks())
    }

    pub const fn timeline(self) -> FrameTimeline<'source> {
        self.inner.frames().timeline()
    }

    pub fn frame_at_ticks(self, elapsed_ticks: u64) -> Option<FramePosition> {
        self.timeline().locate(elapsed_ticks)
    }

    pub const fn group_workspace_len(self) -> usize {
        self.inner.group_workspace_len()
    }

    pub const fn canvas_requirements(self) -> mirx::image::BufferRequirements {
        self.inner.canvas_requirements()
    }

    pub const fn workspace_requirements(self) -> mirx::image::BufferRequirements {
        self.inner.workspace_requirements()
    }

    pub const fn backup_requirements(self) -> mirx::image::BufferRequirements {
        self.inner.backup_requirements()
    }

    pub const fn decode_request(self) -> mirx::image::DecodeRequest {
        self.inner.request()
    }

    pub const fn input_sync(self) -> mirx::image::CacheSync {
        self.inner.input_sync()
    }

    pub const fn output_sync(self) -> mirx::image::CacheSync {
        self.inner.output_sync()
    }

    pub const fn input_alignment(self) -> mirx::types::ByteAlignment {
        self.inner.input_alignment()
    }

    pub const fn input_addresses_are_aligned(self) -> bool {
        self.inner.input_addresses_are_aligned()
    }

    /// Binds reusable playback storage without reading encoded DATA again.
    pub fn bind<'storage>(
        self,
        storage: MirxFramesStorage<'source, 'storage>,
    ) -> Result<MirxFramesSession<'source, 'storage>, MirxFramesError> {
        let MirxFramesStorage {
            groups,
            canvas,
            workspace,
            backup,
        } = storage;
        let timeline = self.timeline();
        Ok(MirxFramesSession {
            inner: self.inner.bind(PlaybackStorage {
                groups,
                canvas,
                workspace,
                backup,
            })?,
            timeline,
        })
    }
}

/// Stateful MIRX frame decoder borrowing all mutable playback storage.
pub struct MirxFramesSession<'source, 'storage> {
    inner: FrameSession<'source, 'storage>,
    timeline: FrameTimeline<'source>,
}

impl MirxFramesSession<'_, '_> {
    pub const fn current_frame(&self) -> Option<u32> {
        self.inner.current_frame()
    }

    pub const fn decode_request(&self) -> mirx::image::DecodeRequest {
        self.inner.request()
    }

    pub const fn input_sync(&self) -> mirx::image::CacheSync {
        self.inner.input_sync()
    }

    pub const fn output_sync(&self) -> mirx::image::CacheSync {
        self.inner.output_sync()
    }

    pub fn reset(&mut self) {
        self.inner.reset();
    }

    /// Presents one frame as a borrowed render texture.
    pub fn present(&mut self, frame: u32) -> Result<Texture<'_>, MirxFramesError> {
        let surface = self.inner.present(frame)?;
        super::texture::texture_from_surface(surface).map_err(Into::into)
    }

    /// Resolves an absolute sequence tick and presents its frame.
    ///
    /// `Ok(None)` means a finite play count has completed.
    pub fn present_at(
        &mut self,
        elapsed_ticks: u64,
    ) -> Result<Option<(FramePosition, Texture<'_>)>, MirxFramesError> {
        let Some(position) = self.timeline.locate(elapsed_ticks) else {
            return Ok(None);
        };
        let texture = self.present(position.frame())?;
        Ok(Some((position, texture)))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/render/mirx_frames.rs"]
mod tests;
