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
mod tests {
    use super::*;
    use mirx::{
        Document,
        frames::{FrameEncodingSet, FrameSequence, FramesEncoder},
        image::{ColorDescription, SampleLayout, SurfaceDescriptor, SurfaceRequirements},
    };

    #[repr(align(64))]
    struct Aligned<const N: usize>([u8; N]);

    fn encoded_frames() -> alloc::vec::Vec<u8> {
        let surface =
            SurfaceDescriptor::new(2, 1, SampleLayout::RGB888, ColorDescription::SRGB).unwrap();
        let sequence = FrameSequence::new(2, 1_000, 40).unwrap();
        let mut encoder = FramesEncoder::new(sequence, surface)
            .unwrap()
            .with_profiles(FrameEncodingSet::lossless())
            .unwrap();
        encoder.push(&[10, 20, 30, 40, 50, 60]).unwrap();
        encoder
            .push_with_duration(&[10, 20, 30, 41, 52, 63], 75)
            .unwrap();
        let mut document = Document::new();
        let id = document.push_frames(encoder.finish().unwrap()).unwrap();
        document.set_primary(id).unwrap();
        document.encode(&Default::default()).unwrap()
    }

    #[test]
    fn plan_binds_aligned_storage_and_presents_textures_without_reallocation() {
        let bytes = encoded_frames();
        let options = MirxTextureOptions::new()
            .with_requirements(
                SurfaceRequirements::new()
                    .with_base_alignment(mirx::types::ByteAlignment::new(64).unwrap())
                    .with_width_multiple(64)
                    .with_stride_multiple(64),
            )
            .with_input_memory(mirx::image::MemoryPlacement::Flash)
            .with_output_memory(mirx::image::MemoryPlacement::SharedNoncoherent)
            .with_workspace_memory(mirx::image::MemoryPlacement::SharedCoherent)
            .with_workspace_alignment(mirx::types::ByteAlignment::new(64).unwrap());
        let mut slots = [None];
        let plan = MirxFramesPlan::open(&bytes, options, &mut slots).unwrap();
        assert_eq!(plan.meta().width, 2);
        assert_eq!(plan.frame_count(), 2);
        assert_eq!(plan.timescale_hz(), 1_000);
        assert_eq!(plan.duration_ticks(0), Some(40));
        assert_eq!(plan.duration_ticks(1), Some(75));
        assert_eq!(plan.timeline().cycle_duration_ticks(), 115);
        assert_eq!(plan.frame_at_ticks(39).unwrap().frame(), 0);
        assert_eq!(plan.frame_at_ticks(40).unwrap().frame(), 1);
        assert_eq!(plan.frame_at_ticks(115).unwrap().play(), 1);
        assert_eq!(plan.canvas_requirements().byte_len(), 192);
        assert_eq!(plan.canvas_requirements().base_alignment(), 64);
        assert_eq!(plan.workspace_requirements().base_alignment(), 64);
        assert_eq!(plan.decode_request(), options.decode_request());
        assert_eq!(plan.input_sync(), mirx::image::CacheSync::None);
        assert_eq!(plan.output_sync(), mirx::image::CacheSync::CleanAfterWrite);
        assert!(plan.input_addresses_are_aligned());

        let mut canvas = Aligned([0; 256]);
        let mut workspace = Aligned([0; 64]);
        let mut session = plan
            .bind(MirxFramesStorage {
                groups: &mut slots,
                canvas: &mut canvas.0,
                workspace: &mut workspace.0,
                backup: &mut [],
            })
            .unwrap();
        assert_eq!(session.decode_request(), options.decode_request());
        assert_eq!(
            session.output_sync(),
            mirx::image::CacheSync::CleanAfterWrite
        );
        let first_ptr = {
            let texture = session.present(0).unwrap();
            assert_eq!(texture.stride, 192);
            assert_eq!(texture.buf.as_slice().as_ptr() as usize % 64, 0);
            assert_eq!(&texture.buf.as_slice()[..6], &[10, 20, 30, 40, 50, 60]);
            texture.buf.as_slice().as_ptr()
        };
        let (position, second) = session.present_at(40).unwrap().unwrap();
        assert_eq!(position.frame(), 1);
        assert_eq!(position.elapsed_ticks(), 0);
        assert_eq!(second.buf.as_slice().as_ptr(), first_ptr);
        assert_eq!(&second.buf.as_slice()[..6], &[10, 20, 30, 41, 52, 63]);
        assert_eq!(session.current_frame(), Some(1));
    }

    #[test]
    fn plan_rejects_static_images_and_unsupported_render_layouts() {
        let flat = Document::new_flat(mirx::image::ImageAsset::new(
            1,
            1,
            mirx::image::ColorFormat::RGB565,
            2,
            alloc::borrow::Cow::Borrowed(&[0, 0]),
        ))
        .unwrap()
        .finish()
        .unwrap()
        .into_owned();
        assert!(matches!(
            MirxFramesPlan::open(&flat, MirxTextureOptions::new(), &mut []),
            Err(MirxFramesError::NoFramesChunk)
        ));

        let surface = SurfaceDescriptor::new(
            2,
            2,
            SampleLayout::I420,
            ColorDescription::BT709_YUV_LIMITED,
        )
        .unwrap();
        let sequence = FrameSequence::new(1, 1_000, 40).unwrap();
        let profiles = FrameEncodingSet::lossless()
            .with_rle(false)
            .with_pixel(false)
            .with_lz4(false)
            .with_reversible_frequency(false)
            .with_delta(false);
        let mut encoder = FramesEncoder::new(sequence, surface)
            .unwrap()
            .with_profiles(profiles)
            .unwrap();
        encoder.push(&[1, 2, 3, 4, 5, 6]).unwrap();
        let mut document = Document::new();
        let id = document.push_frames(encoder.finish().unwrap()).unwrap();
        document.set_primary(id).unwrap();
        let bytes = document.encode(&Default::default()).unwrap();
        assert!(matches!(
            MirxFramesPlan::open(&bytes, MirxTextureOptions::new(), &mut [None]),
            Err(MirxFramesError::UnsupportedLayout(SampleLayout::I420))
        ));
    }
}
