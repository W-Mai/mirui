use alloc::{borrow::Cow, vec::Vec};
use core::ops::Range;

use super::demotion::flat_candidate;
use super::payload::{
    ResolvedImagePlanes, ResolvedNodePayload, encode_error_for_image, resolve_flat_record,
    resolve_node_payload,
};
use super::{
    ChunkSet, Document, DocumentState, EncodeError, EncodeOptions, FlatRecord, LayoutPolicy,
    PayloadStorage, PrimaryHintState, TrailingState,
};
use crate::header::FileHeader;
use crate::image::ImageRef;
use crate::media::{MediaPayload, MediaSectionKind};
use crate::payload::image::{ImagePayloadError, ImagePayloadPlan};
use crate::wire::{write_u16_le, write_u32_le};
use crate::{
    CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, ChunkFlags, ChunkType, FILE_HEADER_LEN,
    FLAT_HEADER_LEN, Layout, PrimaryHints, VERSION_MAJOR, VERSION_MINOR, crc32,
};

#[cfg(test)]
use crate::ColorFormat;
#[cfg(test)]
use crate::image::RawImageView;

const CONTAINER_ALIGNMENT: u32 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct WirePrimary {
    pub(super) chunk_type: u16,
    pub(super) hints: PrimaryHints,
}

impl WirePrimary {
    const NONE: Self = Self {
        chunk_type: 0,
        hints: PrimaryHints::ZERO,
    };
}

pub(super) type ImageSegments<'a> = ResolvedImagePlanes<'a>;

#[derive(Clone, Copy)]
pub(super) struct SegmentedImagePlan<'a> {
    pub(super) payload: ImagePayloadPlan<'a>,
}

impl<'a> SegmentedImagePlan<'a> {
    fn new(image: ImageSegments<'a>) -> Result<Self, EncodeError> {
        let payload = ImagePayloadPlan::from_planes(image).map_err(|error| match error {
            ImagePayloadError::SizeOverflow
            | ImagePayloadError::Surface(crate::image::ImageEncodeError::SizeOverflow) => {
                EncodeError::SizeOverflow
            }
            _ => EncodeError::InvalidPayload {
                chunk_type: ChunkType::IMAGE,
            },
        })?;
        Ok(Self { payload })
    }
}

#[derive(Clone, Copy)]
pub(super) enum PayloadPlan<'a> {
    Verbatim(&'a [u8]),
    SegmentedImage(SegmentedImagePlan<'a>),
}

impl<'a> PayloadPlan<'a> {
    fn encoded_len(self) -> Result<u32, EncodeError> {
        match self {
            Self::Verbatim(bytes) => checked_payload_len(bytes.len()),
            Self::SegmentedImage(image) => Ok(image.payload.payload_size()),
        }
    }

    fn placement_constraint(
        self,
        chunk_type: ChunkType,
        limits: crate::PayloadLimits,
    ) -> PlacementConstraint<'a> {
        match self {
            Self::Verbatim(payload) if chunk_type == ChunkType::FONT => {
                if crate::font::FontView::open(payload, &limits).is_ok() {
                    PlacementConstraint::Font { payload, limits }
                } else {
                    PlacementConstraint::Chunk(CONTAINER_ALIGNMENT)
                }
            }
            Self::Verbatim(payload) if chunk_type == ChunkType::IMAGE => {
                let media = MediaPayload::open(payload).ok();
                match media.and_then(|media| media.section(MediaSectionKind::DATA)) {
                    Some(data) => {
                        let mut alignment = crate::ByteAlignment::new(CONTAINER_ALIGNMENT)
                            .expect("container alignment is valid");
                        match ImageRef::open(payload) {
                            Ok(ImageRef::Raw(view)) => {
                                for plane in view.planes() {
                                    alignment = alignment.max(plane.memory().required_alignment());
                                }
                            }
                            Ok(ImageRef::Encoded(view)) => {
                                if let Ok(required) = view.input_alignment() {
                                    alignment = alignment.max(required);
                                }
                            }
                            Err(_) => {}
                        }
                        PlacementConstraint::AlignedMedia {
                            chunk_type,
                            payload,
                            data_offset: data.descriptor().offset(),
                            alignment,
                        }
                    }
                    None => PlacementConstraint::Chunk(CONTAINER_ALIGNMENT),
                }
            }
            Self::Verbatim(payload) if chunk_type == ChunkType::FRAMES => {
                let frames = crate::frames::FramesView::open(payload, &limits).ok();
                let data = frames.and_then(|frames| {
                    frames
                        .media()
                        .section(MediaSectionKind::DATA)
                        .map(|data| (frames, data))
                });
                match data {
                    Some((frames, data)) => match frames.input_alignment() {
                        Ok(alignment) => PlacementConstraint::AlignedMedia {
                            chunk_type,
                            payload,
                            data_offset: data.descriptor().offset(),
                            alignment: crate::ByteAlignment::new(CONTAINER_ALIGNMENT)
                                .expect("container alignment is valid")
                                .max(alignment),
                        },
                        Err(_) => PlacementConstraint::Chunk(CONTAINER_ALIGNMENT),
                    },
                    None => PlacementConstraint::Chunk(CONTAINER_ALIGNMENT),
                }
            }
            Self::Verbatim(_) | Self::SegmentedImage(_) => {
                PlacementConstraint::Chunk(CONTAINER_ALIGNMENT)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlacementConstraint<'a> {
    Chunk(u32),
    Font {
        payload: &'a [u8],
        limits: crate::PayloadLimits,
    },
    AlignedMedia {
        chunk_type: ChunkType,
        payload: &'a [u8],
        data_offset: u32,
        alignment: crate::ByteAlignment,
    },
}

impl PlacementConstraint<'_> {
    fn place(self, cursor: u32) -> Result<u32, EncodeError> {
        match self {
            Self::Chunk(alignment) => align_up(cursor, alignment),
            Self::Font { payload, limits } => crate::font::FontView::open(payload, &limits)
                .and_then(|view| view.aligned_file_offset(cursor, CONTAINER_ALIGNMENT))
                .map_err(|_| EncodeError::InvalidPayload {
                    chunk_type: ChunkType::FONT,
                }),
            Self::AlignedMedia {
                chunk_type,
                payload,
                data_offset,
                alignment,
            } => {
                let candidate = align_relative(cursor, data_offset, alignment.get())?;
                MediaPayload::open(payload)
                    .and_then(|media| media.validate_file_alignment(candidate, alignment))
                    .map_err(|_| EncodeError::InvalidPayload { chunk_type })?;
                Ok(candidate)
            }
        }
    }
}

pub(super) struct FlatLayoutPlan<'a> {
    pub(super) image: ImageSegments<'a>,
    pub(super) file_size: u32,
    output_len: usize,
}

impl FlatLayoutPlan<'_> {
    pub(super) const fn output_len(&self) -> usize {
        self.output_len
    }
}

#[derive(Clone, Copy)]
enum ChunkPlanSource<'a> {
    Nodes {
        promoted: Option<SegmentedImagePlan<'a>>,
    },
    FlatImage(SegmentedImagePlan<'a>),
}

pub(super) struct ChunkLayoutPlan<'document, 'source> {
    document: &'document Document<'source>,
    source: ChunkPlanSource<'document>,
    chunk_count: u16,
    promoted_index: u16,
    chunk_table_offset: u32,
    table_end: u32,
    file_size: u32,
    primary: WirePrimary,
}

impl<'document, 'source> ChunkLayoutPlan<'document, 'source> {
    fn from_nodes(
        document: &'document Document<'source>,
        chunks: &'document ChunkSet<'source>,
    ) -> Result<Self, EncodeError> {
        let chunk_count = checked_chunk_count(chunks.chunks.len())?;
        let tag_index = chunks
            .chunks
            .iter()
            .position(|node| matches!(node.payload, PayloadStorage::PromotedFlat));
        debug_assert_eq!(chunks.promoted_flat.is_some(), tag_index.is_some());
        debug_assert_eq!(
            chunks
                .chunks
                .iter()
                .filter(|node| matches!(node.payload, PayloadStorage::PromotedFlat))
                .count(),
            usize::from(chunks.promoted_flat.is_some())
        );
        let primary = lower_primary(document, chunks, tag_index)?;
        validate_capabilities(chunks)?;
        let (promoted, promoted_index) = match (chunks.promoted_flat.as_ref(), tag_index) {
            (Some(record), Some(index)) => {
                let index = u16::try_from(index)
                    .expect("chunk count checked before promoted index lowering");
                let image = resolve_flat_record(document, record)
                    .map_err(encode_error_for_image)
                    .and_then(SegmentedImagePlan::new)?;
                (Some(image), index)
            }
            (None, None) => (None, u16::MAX),
            _ => unreachable!("promoted payload tag and sidecar must exist together"),
        };
        Self::finish(
            document,
            ChunkPlanSource::Nodes { promoted },
            chunk_count,
            primary,
            promoted_index,
        )
    }

    fn from_flat(
        document: &'document Document<'source>,
        record: &'document FlatRecord<'source>,
    ) -> Result<Self, EncodeError> {
        let image = resolve_flat_record(document, record).map_err(encode_error_for_image)?;
        let segmented = SegmentedImagePlan::new(image)?;
        let primary = WirePrimary {
            chunk_type: ChunkType::IMAGE.raw(),
            hints: PrimaryHints::new(
                crate::image::SampleLayout::from_color_format(image.format),
                image.width,
                image.height,
                image.stride,
            ),
        };
        Self::finish(
            document,
            ChunkPlanSource::FlatImage(segmented),
            1,
            primary,
            u16::MAX,
        )
    }

    fn finish(
        document: &'document Document<'source>,
        source: ChunkPlanSource<'document>,
        chunk_count: u16,
        primary: WirePrimary,
        promoted_index: u16,
    ) -> Result<Self, EncodeError> {
        let chunk_table_offset = CHUNK_FILE_HEADER_LEN as u32;
        let table_size = u32::from(chunk_count)
            .checked_mul(CHUNK_TABLE_ENTRY_LEN as u32)
            .ok_or(EncodeError::SizeOverflow)?;
        let table_end = chunk_table_offset
            .checked_add(table_size)
            .ok_or(EncodeError::SizeOverflow)?;
        let mut plan = Self {
            document,
            source,
            chunk_count,
            promoted_index,
            chunk_table_offset,
            table_end,
            file_size: table_end,
            primary,
        };

        let mut cursor = table_end;
        for index in 0..chunk_count {
            let placement = plan.checked_placement(index, cursor)?;
            cursor = placement.end_offset;
        }
        plan.file_size = cursor;
        usize::try_from(cursor).map_err(|_| EncodeError::SizeOverflow)?;
        Ok(plan)
    }

    fn payload_at<'plan>(&'plan self, index: u16) -> Result<PayloadPlan<'plan>, EncodeError> {
        match self.source {
            ChunkPlanSource::FlatImage(image) => {
                debug_assert_eq!(index, 0);
                Ok(PayloadPlan::SegmentedImage(image))
            }
            ChunkPlanSource::Nodes { promoted } => {
                let DocumentState::Chunk(chunks) = &self.document.state else {
                    unreachable!("node-backed layout plan requires CHUNK state");
                };
                let node = &chunks.chunks[index as usize];
                if matches!(node.payload, PayloadStorage::PromotedFlat) {
                    let promoted =
                        promoted.expect("promoted payload tag requires a planned sidecar");
                    debug_assert_eq!(self.promoted_index, index);
                    return Ok(PayloadPlan::SegmentedImage(promoted));
                }
                match resolve_node_payload(self.document, node).map_err(encode_error_for_image)? {
                    ResolvedNodePayload::Contiguous { bytes, .. } => {
                        Ok(PayloadPlan::Verbatim(bytes))
                    }
                    ResolvedNodePayload::PromotedImage(_) => {
                        unreachable!("promoted payload handled from the cached layout plan")
                    }
                }
            }
        }
    }

    fn descriptor_at(&self, index: u16) -> (ChunkType, ChunkFlags) {
        match self.source {
            ChunkPlanSource::FlatImage(_) => (ChunkType::IMAGE, ChunkFlags::NONE),
            ChunkPlanSource::Nodes { .. } => {
                let DocumentState::Chunk(chunks) = &self.document.state else {
                    unreachable!("node-backed layout plan requires CHUNK state");
                };
                let node = &chunks.chunks[index as usize];
                (node.chunk_type, node.flags)
            }
        }
    }

    fn checked_placement<'plan>(
        &'plan self,
        index: u16,
        cursor: u32,
    ) -> Result<ChunkPlacement<'plan>, EncodeError> {
        let payload = self.payload_at(index)?;
        let (chunk_type, flags) = self.descriptor_at(index);
        let chunk_size = payload.encoded_len()?;
        let chunk_offset = payload
            .placement_constraint(chunk_type, self.document.payload_limits)
            .place(cursor)?;
        let end_offset = checked_payload_end(chunk_offset, chunk_size)?;
        let table_entry_offset = self
            .chunk_table_offset
            .checked_add(
                u32::from(index)
                    .checked_mul(CHUNK_TABLE_ENTRY_LEN as u32)
                    .ok_or(EncodeError::SizeOverflow)?,
            )
            .ok_or(EncodeError::SizeOverflow)?;
        let table_entry_offset =
            usize::try_from(table_entry_offset).map_err(|_| EncodeError::SizeOverflow)?;
        let cursor_usize = usize::try_from(cursor).map_err(|_| EncodeError::SizeOverflow)?;
        let chunk_start = usize::try_from(chunk_offset).map_err(|_| EncodeError::SizeOverflow)?;
        let chunk_end = usize::try_from(end_offset).map_err(|_| EncodeError::SizeOverflow)?;

        Ok(ChunkPlacement {
            index,
            table_entry_offset,
            leading_padding: cursor_usize..chunk_start,
            chunk_type,
            flags,
            chunk_offset,
            chunk_size,
            output_range: chunk_start..chunk_end,
            payload,
            end_offset,
        })
    }

    pub(super) fn output_len(&self) -> usize {
        usize::try_from(self.file_size)
            .expect("validated CHUNK file size must fit the output address space")
    }

    pub(super) const fn chunk_count(&self) -> u16 {
        self.chunk_count
    }

    pub(super) const fn chunk_table_offset(&self) -> u32 {
        self.chunk_table_offset
    }

    pub(super) const fn table_end(&self) -> u32 {
        self.table_end
    }

    pub(super) const fn file_size(&self) -> u32 {
        self.file_size
    }

    pub(super) const fn primary(&self) -> WirePrimary {
        self.primary
    }

    pub(super) fn placements(&self) -> PlacementCursor<'_, 'document, 'source> {
        PlacementCursor {
            plan: self,
            index: 0,
            cursor: self.table_end,
        }
    }
}

pub(super) struct ChunkPlacement<'a> {
    pub(super) index: u16,
    pub(super) table_entry_offset: usize,
    pub(super) leading_padding: Range<usize>,
    pub(super) chunk_type: ChunkType,
    pub(super) flags: ChunkFlags,
    pub(super) chunk_offset: u32,
    pub(super) chunk_size: u32,
    pub(super) output_range: Range<usize>,
    pub(super) payload: PayloadPlan<'a>,
    end_offset: u32,
}

pub(super) struct PlacementCursor<'plan, 'document, 'source> {
    plan: &'plan ChunkLayoutPlan<'document, 'source>,
    index: u16,
    cursor: u32,
}

impl<'plan, 'document, 'source> Iterator for PlacementCursor<'plan, 'document, 'source> {
    type Item = ChunkPlacement<'plan>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.plan.chunk_count {
            return None;
        }
        let placement = self
            .plan
            .checked_placement(self.index, self.cursor)
            .expect("validated immutable CHUNK layout must replay exactly");
        self.index += 1;
        self.cursor = placement.end_offset;
        Some(placement)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = usize::from(self.plan.chunk_count - self.index);
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for PlacementCursor<'_, '_, '_> {}

pub(super) enum LayoutPlan<'document, 'source> {
    Flat(FlatLayoutPlan<'document>),
    Chunk(ChunkLayoutPlan<'document, 'source>),
}

impl LayoutPlan<'_, '_> {
    pub(super) fn output_len(&self) -> usize {
        match self {
            Self::Flat(plan) => plan.output_len(),
            Self::Chunk(plan) => plan.output_len(),
        }
    }
}

impl<'source> Document<'source> {
    /// Finishes the document, preserving an unchanged source byte-for-byte.
    ///
    /// An unchanged borrowed source remains borrowed, and an unchanged owned
    /// source returns its original allocation. New or modified documents are
    /// encoded with the default options.
    pub fn finish(self) -> Result<Cow<'source, [u8]>, EncodeError> {
        if !self.dirty && self.origin.source().is_some() {
            return Ok(self
                .origin
                .into_cow()
                .expect("source-backed origin must yield its exact bytes"));
        }

        self.encode(&EncodeOptions::new()).map(Cow::Owned)
    }

    /// Returns the exact output length after checking the selected layout.
    pub fn encoded_len(&self, options: &EncodeOptions) -> Result<usize, EncodeError> {
        Ok(self.layout_plan_with(options)?.output_len())
    }

    /// Encodes the document into caller-provided storage.
    ///
    /// The output remains unchanged when validation fails or when `out` is too
    /// short. On success, bytes after the returned encoded length are untouched.
    pub fn encode_into(
        &self,
        out: &mut [u8],
        options: &EncodeOptions,
    ) -> Result<usize, EncodeError> {
        let plan = self.layout_plan_with(options)?;
        let needed = plan.output_len();
        if out.len() < needed {
            return Err(EncodeError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }

        let target = &mut out[..needed];
        target.fill(0);
        emit_layout(&plan, target);
        Ok(needed)
    }

    /// Encodes the document into one exactly sized output allocation.
    pub fn encode(&self, options: &EncodeOptions) -> Result<Vec<u8>, EncodeError> {
        let plan = self.layout_plan_with(options)?;
        let needed = plan.output_len();
        let mut out = Vec::new();
        out.try_reserve_exact(needed)
            .map_err(|_| EncodeError::AllocationFailed)?;
        out.resize(needed, 0);
        emit_layout(&plan, &mut out);
        Ok(out)
    }

    pub(super) fn layout_plan_with<'document>(
        &'document self,
        options: &EncodeOptions,
    ) -> Result<LayoutPlan<'document, 'source>, EncodeError> {
        ensure_rewrite_allowed(self)?;

        match (&self.state, options.layout_policy()) {
            (
                DocumentState::Flat(record),
                LayoutPolicy::PreserveOrPromote
                | LayoutPolicy::SmallestRepresentable
                | LayoutPolicy::ForceFlat,
            ) => Ok(LayoutPlan::Flat(plan_flat(self, record)?)),
            (DocumentState::Flat(record), LayoutPolicy::ForceChunk) => {
                Ok(LayoutPlan::Chunk(ChunkLayoutPlan::from_flat(self, record)?))
            }
            (
                DocumentState::Chunk(chunks),
                LayoutPolicy::PreserveOrPromote | LayoutPolicy::ForceChunk,
            ) => Ok(LayoutPlan::Chunk(ChunkLayoutPlan::from_nodes(
                self, chunks,
            )?)),
            (DocumentState::Chunk(chunks), LayoutPolicy::SmallestRepresentable) => {
                match flat_candidate(self, chunks) {
                    Ok(candidate) => Ok(LayoutPlan::Flat(plan_flat_image(candidate.image)?)),
                    Err(_) => Ok(LayoutPlan::Chunk(ChunkLayoutPlan::from_nodes(
                        self, chunks,
                    )?)),
                }
            }
            (DocumentState::Chunk(chunks), LayoutPolicy::ForceFlat) => {
                let candidate = flat_candidate(self, chunks)
                    .map_err(|_| EncodeError::NotRepresentableAsFlat)?;
                Ok(LayoutPlan::Flat(plan_flat_image(candidate.image)?))
            }
        }
    }
}

fn emit_layout(plan: &LayoutPlan<'_, '_>, out: &mut [u8]) {
    debug_assert_eq!(out.len(), plan.output_len());
    match plan {
        LayoutPlan::Flat(plan) => emit_flat(plan, out),
        LayoutPlan::Chunk(plan) => emit_chunk(plan, out),
    }
}

fn emit_flat(plan: &FlatLayoutPlan<'_>, out: &mut [u8]) {
    debug_assert_eq!(out.len(), plan.output_len());
    debug_assert_eq!(usize::try_from(plan.file_size), Ok(out.len()));
    emit_file_header(Layout::Flat, out);

    let image = plan.image;
    out[8] = image.format.to_u8();
    write_u32_le(out, 12, image.width);
    write_u32_le(out, 16, image.height);
    write_u32_le(out, 20, image.stride);
    write_u32_le(out, 24, crc32(&out[..24]));

    let planes = &mut out[FLAT_HEADER_LEN..];
    let (main, extra) = planes.split_at_mut(image.main.len());
    main.copy_from_slice(image.main);
    match image.extra {
        Some(source) => extra.copy_from_slice(source),
        None => debug_assert!(extra.is_empty()),
    }
    debug_assert_eq!(usize::try_from(image.main_size), Ok(main.len()));
    debug_assert_eq!(usize::try_from(image.extra_size), Ok(extra.len()));
}

fn emit_chunk(plan: &ChunkLayoutPlan<'_, '_>, out: &mut [u8]) {
    debug_assert_eq!(out.len(), plan.output_len());
    debug_assert_eq!(usize::try_from(plan.file_size()), Ok(out.len()));
    debug_assert!(plan.table_end() <= plan.file_size());
    emit_file_header(Layout::Chunk, out);

    write_u16_le(out, 8, plan.chunk_count());
    write_u32_le(out, 12, plan.chunk_table_offset());
    write_u32_le(out, 16, plan.file_size());
    let primary = plan.primary();
    write_u16_le(out, 20, primary.chunk_type);
    write_u16_le(out, 22, primary.hints.sample_layout().raw());
    write_u32_le(out, 24, primary.hints.width());
    write_u32_le(out, 28, primary.hints.height());
    write_u32_le(out, 32, primary.hints.stride());
    write_u32_le(out, 40, crc32(&out[..40]));

    for (expected_index, placement) in plan.placements().enumerate() {
        debug_assert_eq!(usize::from(placement.index), expected_index);
        debug_assert!(
            out[placement.leading_padding.clone()]
                .iter()
                .all(|&byte| byte == 0)
        );

        let entry = placement.table_entry_offset;
        write_u16_le(out, entry, placement.chunk_type.raw());
        write_u16_le(out, entry + 2, placement.flags.bits());
        write_u32_le(out, entry + 4, placement.chunk_offset);
        write_u32_le(out, entry + 8, placement.chunk_size);

        let payload = &mut out[placement.output_range];
        match placement.payload {
            PayloadPlan::Verbatim(source) => payload.copy_from_slice(source),
            PayloadPlan::SegmentedImage(image) => emit_segmented_image(image, payload),
        }
    }
}

fn emit_segmented_image(plan: SegmentedImagePlan<'_>, out: &mut [u8]) {
    debug_assert_eq!(plan.payload.encoded_len(), out.len());
    plan.payload.emit_payload(out);
}

fn emit_file_header(layout: Layout, out: &mut [u8]) {
    let header = FileHeader {
        version_major: VERSION_MAJOR,
        version_minor: VERSION_MINOR,
        layout,
        flags: 0,
    };
    let mut prefix = [0; FILE_HEADER_LEN];
    header.write_into(&mut prefix);
    out[..FILE_HEADER_LEN].copy_from_slice(&prefix);
}

fn ensure_rewrite_allowed(document: &Document<'_>) -> Result<(), EncodeError> {
    if matches!(document.trailing, TrailingState::Preserved) {
        return Err(EncodeError::PreservedTrailingBytesReadOnly);
    }
    Ok(())
}

fn checked_chunk_count(count: usize) -> Result<u16, EncodeError> {
    u16::try_from(count).map_err(|_| EncodeError::TooManyChunks { count })
}

fn checked_payload_len(len: usize) -> Result<u32, EncodeError> {
    u32::try_from(len).map_err(|_| EncodeError::SizeOverflow)
}

fn checked_payload_end(offset: u32, size: u32) -> Result<u32, EncodeError> {
    offset.checked_add(size).ok_or(EncodeError::SizeOverflow)
}

fn lower_primary(
    document: &Document<'_>,
    chunks: &ChunkSet<'_>,
    promoted_index: Option<usize>,
) -> Result<WirePrimary, EncodeError> {
    let Some(primary) = chunks.primary else {
        return Ok(WirePrimary::NONE);
    };
    let node = chunks
        .chunks
        .iter()
        .find(|node| node.id == primary)
        .expect("document primary must identify a live node");
    if matches!(chunks.primary_hints, PrimaryHintState::Missing) {
        return Err(EncodeError::PrimaryHintsRequired {
            chunk_type: node.chunk_type,
        });
    }
    debug_assert_eq!(
        chunks
            .chunks
            .iter()
            .find(|candidate| candidate.chunk_type == node.chunk_type)
            .map(|candidate| candidate.id),
        Some(primary),
    );
    let hints = if promoted_index.and_then(|index| chunks.chunks.get(index).map(|node| node.id))
        == Some(primary)
        && matches!(chunks.primary_hints, PrimaryHintState::Derived)
    {
        let record = chunks
            .promoted_flat
            .as_ref()
            .expect("promoted payload tag requires its FLAT sidecar");
        PrimaryHints::new(
            crate::image::SampleLayout::from_color_format(record.image.format),
            record.image.width,
            record.image.height,
            record.image.stride,
        )
    } else {
        document.primary_hints()
    };
    Ok(WirePrimary {
        chunk_type: node.chunk_type.raw(),
        hints,
    })
}

fn validate_capabilities(chunks: &ChunkSet<'_>) -> Result<(), EncodeError> {
    for (index, node) in chunks.chunks.iter().enumerate() {
        let index = u16::try_from(index).expect("chunk count checked before capability scan");
        let reserved = node.flags.bits() & !ChunkFlags::CRITICAL.bits();
        if reserved != 0 && !node.capability.preserves_reserved_bits() {
            return Err(EncodeError::ReservedFlagBits {
                index,
                chunk_type: node.chunk_type,
                bits: reserved,
            });
        }
        if !node.capability.is_relocatable() {
            return Err(EncodeError::RelocationAssumptionRequired {
                index,
                chunk_type: node.chunk_type,
            });
        }
        if node.flags.is_critical() && !node.capability.critical_understood() {
            return Err(EncodeError::CriticalAssumptionRequired {
                index,
                chunk_type: node.chunk_type,
            });
        }
    }
    Ok(())
}

fn plan_flat<'document>(
    document: &'document Document<'_>,
    record: &'document FlatRecord<'_>,
) -> Result<FlatLayoutPlan<'document>, EncodeError> {
    let image = resolve_flat_record(document, record).map_err(encode_error_for_image)?;
    plan_flat_image(image)
}

fn plan_flat_image(image: ResolvedImagePlanes<'_>) -> Result<FlatLayoutPlan<'_>, EncodeError> {
    let file_size = checked_flat_file_size(image.main_size, image.extra_size)?;
    let output_len = usize::try_from(file_size).map_err(|_| EncodeError::SizeOverflow)?;
    Ok(FlatLayoutPlan {
        image,
        file_size,
        output_len,
    })
}

fn checked_flat_file_size(main_size: u32, extra_size: u32) -> Result<u32, EncodeError> {
    (FLAT_HEADER_LEN as u32)
        .checked_add(main_size)
        .and_then(|size| size.checked_add(extra_size))
        .ok_or(EncodeError::SizeOverflow)
}

fn align_up(value: u32, alignment: u32) -> Result<u32, EncodeError> {
    debug_assert!(alignment.is_power_of_two());
    let padding = (alignment - value % alignment) % alignment;
    value.checked_add(padding).ok_or(EncodeError::SizeOverflow)
}

fn align_relative(value: u32, relative: u32, alignment: u32) -> Result<u32, EncodeError> {
    debug_assert!(alignment.is_power_of_two());
    let alignment = u64::from(alignment);
    let remainder = (u64::from(value) % alignment + u64::from(relative) % alignment) % alignment;
    let padding = (alignment - remainder) % alignment;
    let padding = u32::try_from(padding).map_err(|_| EncodeError::SizeOverflow)?;
    value.checked_add(padding).ok_or(EncodeError::SizeOverflow)
}

#[cfg(test)]
#[path = "../../tests/unit/document/writer/tests.rs"]
mod tests;
