use alloc::boxed::Box;
use core::{
    ops::Range,
    task::{Context, Poll, ready},
};

use super::{DecodeBuffer, DecodePool, Pool, ReadRegion, source_error};
use crate::{
    core::resource::{AssetError, AssetSource, LoadBudget, LoadContext, LoadError},
    render::{RenderError, budget::Allocation},
};
use mirx::{
    image::{
        EncodedImageLayout, EncodedImageScan, ImageValidation, Region, SurfaceDescriptor,
        SurfaceRequirements,
    },
    reader::{MediaSectionKind, PayloadLimits},
};

const KINDS: [MediaSectionKind; 3] = [
    MediaSectionKind::CODINGS,
    MediaSectionKind::UNIT_GROUPS,
    MediaSectionKind::UNIT_INDEX,
];

struct Metadata {
    surface: SurfaceDescriptor,
    data: Range<usize>,
    sections: [Option<(usize, Range<usize>)>; 3],
    bytes: DecodeBuffer,
}

impl Metadata {
    fn layout(&self) -> Result<EncodedImageLayout<'_>, mirx::image::EncodedImageError> {
        let body = |index: usize| {
            self.sections[index]
                .as_ref()
                .map(|(_, range)| &self.bytes.bytes[range.clone()])
        };
        EncodedImageLayout::new(
            self.surface,
            body(0).expect("coding section"),
            body(1),
            body(2),
            self.data.len() as u32,
            Some(self.data.start as u32),
        )
    }
}

pub(crate) struct EncodedImage {
    metadata: Metadata,
    input: Option<DecodeBuffer>,
    workspace: Option<DecodeBuffer>,
    _allocation: Allocation,
}

pub(crate) struct EncodedPreparation {
    section: usize,
    group: usize,
    ordinal: usize,
    validation: ImageValidation<'static>,
    admitted: bool,
}

impl Default for EncodedPreparation {
    fn default() -> Self {
        Self {
            section: 0,
            group: 0,
            ordinal: 0,
            validation: ImageValidation::new(&PayloadLimits::EMBEDDED),
            admitted: false,
        }
    }
}

fn invalid(_: impl core::fmt::Debug) -> LoadError {
    LoadError::Failed("invalid MIRX encoded image")
}

fn ensure_buffer(
    slot: &mut Option<DecodeBuffer>,
    size: usize,
    pool: &Pool,
    cx: &mut Context<'_>,
) -> Poll<Result<(), RenderError>> {
    if slot
        .as_ref()
        .is_some_and(|buffer| buffer.bytes.len() >= size)
    {
        return Poll::Ready(Ok(()));
    }
    // These are private mutable workspaces, never published or pinned by a draw.
    let mut pool = ready!(pool.poll_take(cx)).map_err(super::super::state_error)?;
    *slot = None;
    *slot = Some(pool.buffer(size)?);
    Poll::Ready(Ok(()))
}

impl EncodedImage {
    pub(crate) fn new(
        scan: &EncodedImageScan,
        base: usize,
        pool: &mut DecodePool,
    ) -> Result<Box<Self>, LoadError> {
        let surface = scan.surface().map_err(invalid)?;
        let data = scan
            .section(MediaSectionKind::DATA)
            .ok_or(AssetError::InvalidSource)?;
        let data_start = base
            .checked_add(data.offset() as usize)
            .ok_or(AssetError::OutOfBounds)?;
        let data_end = data_start
            .checked_add(data.size() as usize)
            .filter(|end| *end <= u32::MAX as usize)
            .ok_or(AssetError::OutOfBounds)?;
        let mut size = 0usize;
        let mut sections = core::array::from_fn(|_| None);
        for (index, kind) in KINDS.into_iter().enumerate() {
            if let Some(section) = scan.section(kind) {
                let end = size
                    .checked_add(section.size() as usize)
                    .ok_or(AssetError::Capacity)?;
                sections[index] = Some((base + section.offset() as usize, size..end));
                size = end;
            }
        }
        let allocation = pool
            .workspace(core::mem::size_of::<Self>())
            .map_err(|_| AssetError::Capacity)?;
        let bytes = pool.buffer(size).map_err(|_| AssetError::Capacity)?;
        Ok(Box::new(Self {
            metadata: Metadata {
                surface,
                data: data_start..data_end,
                sections,
                bytes,
            },
            input: None,
            workspace: None,
            _allocation: allocation,
        }))
    }

    pub(crate) fn surface(&self) -> SurfaceDescriptor {
        self.metadata.surface
    }
    pub(crate) fn data_range(&self) -> Range<usize> {
        self.metadata.data.clone()
    }

    pub(crate) fn poll_prepare(
        &mut self,
        progress: &mut EncodedPreparation,
        pool: &Pool,
        cx: &mut LoadContext<'_>,
    ) -> Poll<Result<(), LoadError>> {
        while progress.section < self.metadata.sections.len() {
            if let Some((offset, range)) = &self.metadata.sections[progress.section] {
                ready!(cx.poll_read_exact(
                    *offset as u64,
                    &mut self.metadata.bytes.bytes[range.clone()]
                ))?;
            }
            progress.section += 1;
        }
        let layout = self.metadata.layout().map_err(invalid)?;
        if !progress.admitted {
            progress
                .validation
                .validate_layout(layout)
                .map_err(invalid)?;
            progress
                .validation
                .charge_data(self.metadata.data.len() as u32)
                .map_err(invalid)?;
            let mut input_size = 0;
            for index in 0..layout.group_count() {
                let group = layout.group(index).map_err(invalid)?;
                for ordinal in 0..group.len() {
                    input_size =
                        input_size.max(group.unit_range(ordinal).expect("validated ordinal").len());
                }
            }
            ready!(ensure_buffer(
                &mut self.input,
                input_size,
                pool,
                &mut Context::from_waker(cx.waker())
            ))
            .map_err(|_| AssetError::Capacity)?;
            progress.admitted = true;
        }
        let input = &mut self.input.as_mut().expect("admitted input").bytes;
        while progress.group < layout.group_count() {
            let group = layout.group(progress.group).map_err(invalid)?;
            while progress.ordinal < group.len() {
                let range = group
                    .unit_range(progress.ordinal)
                    .expect("validated ordinal");
                let bytes = &mut input[..range.len()];
                ready!(cx.poll_read_exact(
                    (self.metadata.data.start + range.start as usize) as u64,
                    bytes
                ))?;
                let unit = group.unit(progress.ordinal, bytes).map_err(invalid)?;
                progress
                    .validation
                    .validate_unit(progress.group, progress.ordinal, unit)
                    .map_err(invalid)?;
                progress.ordinal += 1;
            }
            progress.group += 1;
            progress.ordinal = 0;
        }
        Poll::Ready(Ok(()))
    }

    pub(super) fn poll_region(
        &mut self,
        source: &mut AssetSource,
        pending: &mut ReadRegion,
        pool: &Pool,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), RenderError>> {
        let layout = self
            .metadata
            .layout()
            .map_err(|_| RenderError::InvalidTexture)?;
        let region = Region::new(
            pending.key.x,
            pending.key.y,
            pending.key.width,
            pending.key.height,
        )
        .map_err(|_| RenderError::InvalidTexture)?;
        let output = layout
            .surface()
            .region_plan(region, SurfaceRequirements::new())
            .map_err(|_| RenderError::InvalidTexture)?;
        let mut budget = LoadBudget::default();
        for group_index in pending.group..layout.group_count() {
            let group = layout
                .group(group_index)
                .map_err(|_| RenderError::InvalidTexture)?;
            let start = pending.ordinal;
            for ordinal in group
                .units_in(region)
                .map_err(|_| RenderError::InvalidTexture)?
                .filter(|ordinal| *ordinal >= start)
            {
                pending.group = group_index;
                pending.ordinal = ordinal;
                let range = group.unit_range(ordinal).expect("selected ordinal");
                let input = &mut self
                    .input
                    .as_mut()
                    .expect("validated input workspace")
                    .bytes[..range.len()];
                while pending.filled < input.len() {
                    let n = ready!(source.poll_read_budgeted(
                        (self.metadata.data.start + range.start as usize + pending.filled) as u64,
                        &mut input[pending.filled..],
                        cx,
                        &mut budget
                    ))
                    .map_err(source_error)?;
                    if n == 0 {
                        return Poll::Ready(Err(RenderError::InvalidTexture));
                    }
                    pending.filled += n;
                }
                let plan = group
                    .unit(ordinal, input)
                    .map_err(|_| RenderError::InvalidTexture)?
                    .decode_plan(SurfaceRequirements::new())
                    .map_err(|_| RenderError::InvalidTexture)?;
                ready!(ensure_buffer(
                    &mut self.workspace,
                    plan.memory_plan().byte_len() as usize,
                    pool,
                    cx,
                ))?;
                let decoded = plan
                    .decode_into(&mut self.workspace.as_mut().expect("admitted workspace").bytes)
                    .map_err(|_| RenderError::InvalidTexture)?;
                decoded
                    .copy_region_into(
                        &mut pending
                            .pixels
                            .get_mut()
                            .ok_or(RenderError::InvalidScope)?
                            .bytes,
                        output,
                    )
                    .map_err(|_| RenderError::InvalidTexture)?;
                pending.filled = 0;
                pending.ordinal = ordinal + 1;
            }
            pending.group = group_index + 1;
            pending.ordinal = 0;
        }
        Poll::Ready(Ok(()))
    }
}
