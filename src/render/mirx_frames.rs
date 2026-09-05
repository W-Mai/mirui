//! Allocation-free MIRX frame playback over caller-owned storage.

use super::texture::{MirxLoadError, MirxTextureOptions, Texture, TextureMeta, map_mirx_format};

/// Failure while opening, planning, or presenting a MIRX frame sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MirxFramesError {
    Read(mirx::ReadError),
    Frames(mirx::FramesError),
    Playback(mirx::FrameDecodeError),
    UnsupportedFormat(mirx::ColorFormat),
    UnsupportedLayout(mirx::image::SampleLayout),
    NoFramesChunk,
    DimensionOverflow,
}

impl From<mirx::ReadError> for MirxFramesError {
    fn from(error: mirx::ReadError) -> Self {
        Self::Read(error)
    }
}

impl From<mirx::FramesError> for MirxFramesError {
    fn from(error: mirx::FramesError) -> Self {
        Self::Frames(error)
    }
}

impl From<mirx::FrameDecodeError> for MirxFramesError {
    fn from(error: mirx::FrameDecodeError) -> Self {
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
    inner: mirx::FramesPlaybackPlan<'source>,
    meta: TextureMeta,
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
        let inner = frames.playback_plan(options.requirements(), options.limits(), group_slots)?;
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

    pub const fn timeline(self) -> mirx::FrameTimeline<'source> {
        self.inner.frames().timeline()
    }

    pub fn frame_at_ticks(self, elapsed_ticks: u64) -> Option<mirx::FramePosition> {
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

    /// Binds reusable playback storage without reading encoded DATA again.
    pub fn bind<'storage>(
        self,
        group_slots: &'storage mut [Option<mirx::image::UnitGroup<'source>>],
        canvas: &'storage mut [u8],
        workspace: &'storage mut [u8],
        backup: &'storage mut [u8],
    ) -> Result<MirxFramesSession<'source, 'storage>, MirxFramesError> {
        let timeline = self.timeline();
        Ok(MirxFramesSession {
            inner: self.inner.bind(group_slots, canvas, workspace, backup)?,
            timeline,
        })
    }
}

/// Stateful MIRX frame decoder borrowing all mutable playback storage.
pub struct MirxFramesSession<'source, 'storage> {
    inner: mirx::FrameSession<'source, 'storage>,
    timeline: mirx::FrameTimeline<'source>,
}

impl MirxFramesSession<'_, '_> {
    pub const fn current_frame(&self) -> Option<u32> {
        self.inner.current_frame()
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
    ) -> Result<Option<(mirx::FramePosition, Texture<'_>)>, MirxFramesError> {
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
        Document, FrameEncodingSet, FrameSequence, FramesEncoder,
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
        let options = MirxTextureOptions::new().with_requirements(
            SurfaceRequirements::new()
                .with_base_alignment(64)
                .with_width_multiple(64)
                .with_stride_multiple(64),
        );
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

        let mut canvas = Aligned([0; 256]);
        let mut workspace = [0; 16];
        let mut session = plan
            .bind(&mut slots, &mut canvas.0, &mut workspace, &mut [])
            .unwrap();
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
        let flat = mirx::encode_flat(&mirx::FlatImageInput {
            width: 1,
            height: 1,
            stride: 2,
            format: mirx::ColorFormat::RGB565,
            main: &[0, 0],
            extra: None,
        });
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
