use alloc::{borrow::Cow, vec::Vec};
use core::ops::Range;

use super::{
    ChunkNode, ChunkSet, Compatibility, Document, DocumentState, EncodeOptions, FlatRecord,
    LayoutPolicy, PayloadStorage, PrimaryHintState, TrailingState,
};
use crate::wire::read_u32_le;
use crate::{
    CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, ChunkFlags, ChunkType, ColorFormat, EncodeError,
    FILE_HEADER_LEN, FLAT_HEADER_LEN, FileHeader, ImageChunkHeader, ImageView, Layout,
    PrimaryHints, VERSION_MAJOR, VERSION_MINOR, crc32,
};

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

#[derive(Clone, Copy)]
pub(super) struct ImageSegments<'a> {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: ColorFormat,
    pub(super) stride: u32,
    pub(super) main: &'a [u8],
    pub(super) extra: Option<&'a [u8]>,
    pub(super) main_size: u32,
    pub(super) extra_size: u32,
}

#[derive(Clone, Copy)]
pub(super) struct SegmentedImagePlan<'a> {
    pub(super) image: ImageSegments<'a>,
    pub(super) data_offset: u32,
    pub(super) data_size: u32,
    pub(super) payload_size: u32,
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
            Self::SegmentedImage(image) => Ok(image.payload_size),
        }
    }

    fn placement_constraint(self, chunk_type: ChunkType) -> PlacementConstraint<'a> {
        match self {
            Self::Verbatim(payload) if chunk_type == ChunkType::IMAGE => {
                match read_u32_le(payload, 16) {
                    Some(data_offset) => PlacementConstraint::RawImageDataAligned {
                        payload,
                        data_offset,
                        alignment: CONTAINER_ALIGNMENT,
                    },
                    None => PlacementConstraint::ChunkStartAligned(CONTAINER_ALIGNMENT),
                }
            }
            Self::Verbatim(_) | Self::SegmentedImage(_) => {
                PlacementConstraint::ChunkStartAligned(CONTAINER_ALIGNMENT)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlacementConstraint<'a> {
    ChunkStartAligned(u32),
    RawImageDataAligned {
        payload: &'a [u8],
        data_offset: u32,
        alignment: u32,
    },
}

impl PlacementConstraint<'_> {
    fn place(self, cursor: u32) -> Result<u32, EncodeError> {
        match self {
            Self::ChunkStartAligned(alignment) => align_up(cursor, alignment),
            Self::RawImageDataAligned {
                payload,
                data_offset,
                alignment,
            } => match align_relative(cursor, data_offset, alignment) {
                Ok(candidate) if inspect_raw_image_at(payload, candidate).is_some() => {
                    Ok(candidate)
                }
                Ok(_) | Err(_) => align_up(cursor, alignment),
            },
        }
    }
}

fn inspect_raw_image_at<'a>(payload: &'a [u8], payload_offset: u32) -> Option<ImageView<'a>> {
    ImageView::from_chunk_payload(payload, payload_offset).ok()
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
    Nodes,
    FlatImage(SegmentedImagePlan<'a>),
}

pub(super) struct ChunkLayoutPlan<'document, 'source> {
    document: &'document Document<'source>,
    source: ChunkPlanSource<'document>,
    chunk_count: u16,
    chunk_table_offset: u32,
    table_end: u32,
    file_size: u32,
    output_len: usize,
    primary: WirePrimary,
}

impl<'document, 'source> ChunkLayoutPlan<'document, 'source> {
    fn from_nodes(
        document: &'document Document<'source>,
        chunks: &ChunkSet<'source>,
    ) -> Result<Self, EncodeError> {
        let chunk_count = checked_chunk_count(chunks.chunks.len())?;
        let primary = lower_primary(document, chunks)?;
        validate_capabilities(chunks)?;
        Self::finish(document, ChunkPlanSource::Nodes, chunk_count, primary)
    }

    fn from_flat(
        document: &'document Document<'source>,
        record: &FlatRecord,
    ) -> Result<Self, EncodeError> {
        let image = resolve_flat_image(document, record)?;
        let data_offset = ImageChunkHeader::SIZE as u32;
        let data_size = image
            .main_size
            .checked_add(image.extra_size)
            .ok_or(EncodeError::SizeOverflow)?;
        let payload_size = data_offset
            .checked_add(data_size)
            .ok_or(EncodeError::SizeOverflow)?;
        let segmented = SegmentedImagePlan {
            image,
            data_offset,
            data_size,
            payload_size,
        };
        let primary = WirePrimary {
            chunk_type: ChunkType::IMAGE.raw(),
            hints: PrimaryHints::new(
                image.format.to_u8(),
                image.width,
                image.height,
                image.stride,
            ),
        };
        Self::finish(document, ChunkPlanSource::FlatImage(segmented), 1, primary)
    }

    fn finish(
        document: &'document Document<'source>,
        source: ChunkPlanSource<'document>,
        chunk_count: u16,
        primary: WirePrimary,
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
            chunk_table_offset,
            table_end,
            file_size: table_end,
            output_len: 0,
            primary,
        };

        let mut cursor = table_end;
        for index in 0..chunk_count {
            let placement = plan.checked_placement(index, cursor)?;
            cursor = placement.end_offset;
        }
        plan.file_size = cursor;
        plan.output_len = usize::try_from(cursor).map_err(|_| EncodeError::SizeOverflow)?;
        Ok(plan)
    }

    fn payload_at<'plan>(&'plan self, index: u16) -> PayloadPlan<'plan> {
        match self.source {
            ChunkPlanSource::FlatImage(image) => {
                debug_assert_eq!(index, 0);
                PayloadPlan::SegmentedImage(image)
            }
            ChunkPlanSource::Nodes => {
                let DocumentState::Chunk(chunks) = &self.document.state else {
                    unreachable!("node-backed layout plan requires CHUNK state");
                };
                let node = &chunks.chunks[index as usize];
                PayloadPlan::Verbatim(resolve_node_payload(self.document, node))
            }
        }
    }

    fn descriptor_at(&self, index: u16) -> (ChunkType, ChunkFlags) {
        match self.source {
            ChunkPlanSource::FlatImage(_) => (ChunkType::IMAGE, ChunkFlags::NONE),
            ChunkPlanSource::Nodes => {
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
        let payload = self.payload_at(index);
        let (chunk_type, flags) = self.descriptor_at(index);
        let chunk_size = payload.encoded_len()?;
        let chunk_offset = payload.placement_constraint(chunk_type).place(cursor)?;
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

    pub(super) const fn output_len(&self) -> usize {
        self.output_len
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
    pub(super) const fn output_len(&self) -> usize {
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

        self.encode_with(&EncodeOptions::new()).map(Cow::Owned)
    }

    /// Returns the exact output length after checking the selected layout.
    pub fn encoded_len_with(&self, options: &EncodeOptions) -> Result<usize, EncodeError> {
        Ok(self.layout_plan_with(options)?.output_len())
    }

    /// Encodes the document into caller-provided storage.
    ///
    /// The output remains unchanged when validation fails or when `out` is too
    /// short. On success, bytes after the returned encoded length are untouched.
    pub fn encode_into_with(
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
    pub fn encode_with(&self, options: &EncodeOptions) -> Result<Vec<u8>, EncodeError> {
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
                DocumentState::SourceFlat(record),
                LayoutPolicy::PreserveOrPromote
                | LayoutPolicy::SmallestRepresentable
                | LayoutPolicy::ForceFlat,
            ) => Ok(LayoutPlan::Flat(plan_flat(self, record)?)),
            (DocumentState::SourceFlat(record), LayoutPolicy::ForceChunk) => {
                Ok(LayoutPlan::Chunk(ChunkLayoutPlan::from_flat(self, record)?))
            }
            (DocumentState::Chunk(_), LayoutPolicy::ForceFlat) => {
                Err(EncodeError::NotRepresentableAsFlat)
            }
            (
                DocumentState::Chunk(chunks),
                LayoutPolicy::PreserveOrPromote
                | LayoutPolicy::SmallestRepresentable
                | LayoutPolicy::ForceChunk,
            ) => Ok(LayoutPlan::Chunk(ChunkLayoutPlan::from_nodes(
                self, chunks,
            )?)),
            (DocumentState::OpaqueFlat(_), _) => {
                debug_assert!(matches!(self.compatibility, Compatibility::FutureReadOnly));
                Err(EncodeError::FutureSemanticsReadOnly)
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
    write_u32(out, 12, image.width);
    write_u32(out, 16, image.height);
    write_u32(out, 20, image.stride);
    write_u32(out, 24, crc32(&out[..24]));

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

    write_u16(out, 8, plan.chunk_count());
    write_u32(out, 12, plan.chunk_table_offset());
    write_u32(out, 16, plan.file_size());
    let primary = plan.primary();
    write_u16(out, 20, primary.chunk_type);
    out[22] = primary.hints.color_format_raw();
    write_u32(out, 24, primary.hints.width());
    write_u32(out, 28, primary.hints.height());
    write_u32(out, 32, primary.hints.stride());
    write_u32(out, 40, crc32(&out[..40]));

    for (expected_index, placement) in plan.placements().enumerate() {
        debug_assert_eq!(usize::from(placement.index), expected_index);
        debug_assert!(
            out[placement.leading_padding.clone()]
                .iter()
                .all(|&byte| byte == 0)
        );

        let entry = placement.table_entry_offset;
        write_u16(out, entry, placement.chunk_type.raw());
        write_u16(out, entry + 2, placement.flags.bits());
        write_u32(out, entry + 4, placement.chunk_offset);
        write_u32(out, entry + 8, placement.chunk_size);

        let payload = &mut out[placement.output_range];
        match placement.payload {
            PayloadPlan::Verbatim(source) => payload.copy_from_slice(source),
            PayloadPlan::SegmentedImage(image) => emit_segmented_image(image, payload),
        }
    }
}

fn emit_segmented_image(plan: SegmentedImagePlan<'_>, out: &mut [u8]) {
    debug_assert_eq!(usize::try_from(plan.payload_size), Ok(out.len()));
    let image = plan.image;
    write_u32(out, 0, image.width);
    write_u32(out, 4, image.height);
    out[8] = image.format.to_u8();
    write_u32(out, 12, image.stride);
    write_u32(out, 16, plan.data_offset);
    write_u32(out, 20, plan.data_size);
    write_u32(out, 24, image.extra_size);

    let data_offset = usize::try_from(plan.data_offset)
        .expect("validated segmented IMAGE offset must fit the output address space");
    let data = &mut out[data_offset..];
    let data_len = data.len();
    let (main, extra) = data.split_at_mut(image.main.len());
    main.copy_from_slice(image.main);
    match image.extra {
        Some(source) => extra.copy_from_slice(source),
        None => debug_assert!(extra.is_empty()),
    }
    debug_assert_eq!(usize::try_from(plan.data_size), Ok(data_len));
    debug_assert_eq!(usize::try_from(image.main_size), Ok(main.len()));
    debug_assert_eq!(usize::try_from(image.extra_size), Ok(extra.len()));
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

fn write_u16(out: &mut [u8], offset: usize, value: u16) {
    out[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(out: &mut [u8], offset: usize, value: u32) {
    out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn ensure_rewrite_allowed(document: &Document<'_>) -> Result<(), EncodeError> {
    if matches!(document.compatibility, Compatibility::FutureReadOnly) {
        return Err(EncodeError::FutureSemanticsReadOnly);
    }
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
    Ok(WirePrimary {
        chunk_type: node.chunk_type.raw(),
        hints: document.primary_hints(),
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

fn resolve_node_payload<'document>(
    document: &'document Document<'_>,
    node: &'document ChunkNode<'_>,
) -> &'document [u8] {
    match &node.payload {
        PayloadStorage::SourceRange(range) => document
            .origin
            .resolve(*range)
            .expect("validated source range must remain resolvable"),
        PayloadStorage::Borrowed(bytes) => bytes,
        PayloadStorage::Owned(bytes) => bytes.as_slice(),
    }
}

fn resolve_flat_image<'document>(
    document: &'document Document<'_>,
    record: &FlatRecord,
) -> Result<ImageSegments<'document>, EncodeError> {
    let main = document
        .origin
        .resolve(record.main)
        .ok_or(EncodeError::SizeOverflow)?;
    let extra = match record.extra {
        Some(range) => Some(
            document
                .origin
                .resolve(range)
                .ok_or(EncodeError::SizeOverflow)?,
        ),
        None => None,
    };
    let main_size = record
        .image
        .stride
        .checked_mul(record.image.height)
        .ok_or(EncodeError::SizeOverflow)?;
    let extra_size = record
        .image
        .format
        .extra_size(record.image.width, record.image.height, record.image.stride)
        .ok_or(EncodeError::SizeOverflow)?;
    let expected_main = usize::try_from(main_size).map_err(|_| EncodeError::SizeOverflow)?;
    let expected_extra = usize::try_from(extra_size).map_err(|_| EncodeError::SizeOverflow)?;
    if main.len() != expected_main || extra.map_or(0, <[u8]>::len) != expected_extra {
        return Err(EncodeError::InvalidPayload {
            chunk_type: ChunkType::IMAGE,
        });
    }

    Ok(ImageSegments {
        width: record.image.width,
        height: record.image.height,
        format: record.image.format,
        stride: record.image.stride,
        main,
        extra,
        main_size,
        extra_size,
    })
}

fn plan_flat<'document>(
    document: &'document Document<'_>,
    record: &FlatRecord,
) -> Result<FlatLayoutPlan<'document>, EncodeError> {
    let image = resolve_flat_image(document, record)?;
    let file_size = (FLAT_HEADER_LEN as u32)
        .checked_add(image.main_size)
        .and_then(|size| size.checked_add(image.extra_size))
        .ok_or(EncodeError::SizeOverflow)?;
    let output_len = usize::try_from(file_size).map_err(|_| EncodeError::SizeOverflow)?;
    Ok(FlatLayoutPlan {
        image,
        file_size,
        output_len,
    })
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
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;
    use core::mem::{needs_drop, size_of};

    use super::*;
    use crate::document::{
        CompatibilityPolicy, CriticalAssumption, OpenOptions, PayloadInput, RawChunkInput,
        RawChunkPolicy, RawTypePolicy, RelocationAssumption, ReservedBitsPolicy,
    };
    use crate::wire::{read_u16_le, read_u32_le};
    use crate::{
        FlatImageInput, ImageView, Layout, PRIMARY_FORMAT_NONE, Reader, TrailingBytesPolicy,
        VERSION_MINOR, crc32, encode_chunks, encode_flat,
    };

    const CUSTOM_A: ChunkType = match ChunkType::new(0xa001) {
        Some(value) => value,
        None => panic!("nonzero chunk type"),
    };
    const CUSTOM_B: ChunkType = match ChunkType::new(0xb001) {
        Some(value) => value,
        None => panic!("nonzero chunk type"),
    };

    const fn policy(
        relocation: RelocationAssumption,
        critical: CriticalAssumption,
        reserved: ReservedBitsPolicy,
    ) -> RawChunkPolicy {
        RawChunkPolicy {
            relocation,
            critical_semantics: critical,
            reserved_flag_bits: reserved,
        }
    }

    const fn relocatable_policy() -> RawChunkPolicy {
        policy(
            RelocationAssumption::AssumeRelocatable,
            CriticalAssumption::Infer,
            ReservedBitsPolicy::Reject,
        )
    }

    fn raw<'a>(chunk_type: ChunkType, payload: &'a [u8]) -> RawChunkInput<'a> {
        RawChunkInput {
            chunk_type,
            flags: ChunkFlags::NONE,
            payload: PayloadInput::Borrowed(payload),
            policy: relocatable_policy(),
        }
    }

    fn chunk_plan<'document, 'source>(
        document: &'document Document<'source>,
        options: &EncodeOptions,
    ) -> ChunkLayoutPlan<'document, 'source> {
        match document.layout_plan_with(options).unwrap() {
            LayoutPlan::Chunk(plan) => plan,
            LayoutPlan::Flat(_) => panic!("expected CHUNK plan"),
        }
    }

    fn flat_plan<'document>(
        document: &'document Document<'_>,
        options: &EncodeOptions,
    ) -> FlatLayoutPlan<'document> {
        match document.layout_plan_with(options).unwrap() {
            LayoutPlan::Flat(plan) => plan,
            LayoutPlan::Chunk(_) => panic!("expected FLAT plan"),
        }
    }

    fn image_payload(data_offset: u32) -> Vec<u8> {
        let width = 1u32;
        let height = 1u32;
        let format = ColorFormat::A8;
        let stride = format.minimum_stride(width).unwrap();
        let data_size = stride.checked_mul(height).unwrap();
        let payload_len = usize::try_from(data_offset.checked_add(data_size).unwrap()).unwrap();
        let mut payload = vec![0; payload_len];
        payload[0..4].copy_from_slice(&width.to_le_bytes());
        payload[4..8].copy_from_slice(&height.to_le_bytes());
        payload[8] = format.to_u8();
        payload[12..16].copy_from_slice(&stride.to_le_bytes());
        payload[16..20].copy_from_slice(&data_offset.to_le_bytes());
        payload[20..24].copy_from_slice(&data_size.to_le_bytes());
        payload[usize::try_from(data_offset).unwrap()] = 0x5a;
        payload
    }

    fn padded_image_payload(data_offset: u32) -> Vec<u8> {
        let width = 1u32;
        let height = 2u32;
        let format = ColorFormat::A8;
        let stride = format
            .minimum_stride(width)
            .unwrap()
            .checked_add(3)
            .unwrap();
        let data_size = stride.checked_mul(height).unwrap();
        let payload_len = usize::try_from(data_offset.checked_add(data_size).unwrap()).unwrap();
        let mut payload = vec![0; payload_len];
        payload[0..4].copy_from_slice(&width.to_le_bytes());
        payload[4..8].copy_from_slice(&height.to_le_bytes());
        payload[8] = format.to_u8();
        payload[12..16].copy_from_slice(&stride.to_le_bytes());
        payload[16..20].copy_from_slice(&data_offset.to_le_bytes());
        payload[20..24].copy_from_slice(&data_size.to_le_bytes());
        let data_start = usize::try_from(data_offset).unwrap();
        payload[data_start..].copy_from_slice(&[0x11, 0xa1, 0xa2, 0xa3, 0x22, 0xb1, 0xb2, 0xb3]);
        payload
    }

    fn flat_source() -> Vec<u8> {
        let format = ColorFormat::A8;
        let width = 2;
        let height = 2;
        let stride = format.minimum_stride(width).unwrap();
        let main = [0x10, 0x20, 0x30, 0x40];
        encode_flat(&FlatImageInput {
            width,
            height,
            stride,
            format,
            main: &main,
            extra: None,
        })
    }

    fn set_future_minor(source: &mut [u8]) {
        source[5] = VERSION_MINOR + 1;
        let checksum_offset = match Layout::from_u8(source[6]).unwrap() {
            Layout::Flat => 24,
            Layout::Chunk => 40,
        };
        let checksum = crc32(&source[..checksum_offset]);
        source[checksum_offset..checksum_offset + 4].copy_from_slice(&checksum.to_le_bytes());
    }

    fn chunk_set_mut<'document, 'source>(
        document: &'document mut Document<'source>,
    ) -> &'document mut ChunkSet<'source> {
        match &mut document.state {
            DocumentState::Chunk(chunks) => chunks,
            _ => panic!("expected CHUNK document"),
        }
    }

    #[test]
    fn empty_chunk_plan_has_canonical_header_and_zero_primary() {
        let document = Document::new_chunk();
        let options = EncodeOptions::new();
        let plan = chunk_plan(&document, &options);

        assert_eq!(
            document.encoded_len_with(&options),
            Ok(CHUNK_FILE_HEADER_LEN)
        );
        assert_eq!(plan.chunk_count(), 0);
        assert_eq!(plan.chunk_table_offset(), CHUNK_FILE_HEADER_LEN as u32);
        assert_eq!(plan.table_end(), CHUNK_FILE_HEADER_LEN as u32);
        assert_eq!(plan.file_size(), CHUNK_FILE_HEADER_LEN as u32);
        assert_eq!(plan.output_len(), CHUNK_FILE_HEADER_LEN);
        assert_eq!(plan.primary(), WirePrimary::NONE);
        assert_eq!(plan.placements().len(), 0);
        assert!(!needs_drop::<ChunkLayoutPlan<'_, '_>>());
        assert!(size_of::<ChunkLayoutPlan<'_, '_>>() <= 128);
    }

    #[test]
    fn current_flat_stays_flat_or_becomes_one_segmented_primary_image() {
        let format = ColorFormat::I4;
        let width = 3;
        let height = 2;
        let stride = format.minimum_stride(width).unwrap();
        let main = [1, 2, 3, 4];
        let palette = [0xa5; 64];
        let source = encode_flat(&FlatImageInput {
            width,
            height,
            stride,
            format,
            main: &main,
            extra: Some(&palette),
        });
        let document = Document::open(&source).unwrap();

        for policy in [
            LayoutPolicy::PreserveOrPromote,
            LayoutPolicy::SmallestRepresentable,
            LayoutPolicy::ForceFlat,
        ] {
            let options = EncodeOptions::new().with_layout_policy(policy);
            let plan = flat_plan(&document, &options);
            assert_eq!(plan.file_size as usize, source.len());
            assert_eq!(plan.output_len(), source.len());
            assert_eq!(plan.image.main.as_ptr(), source[FLAT_HEADER_LEN..].as_ptr());
            assert_eq!(
                plan.image.extra.unwrap().as_ptr(),
                source[FLAT_HEADER_LEN + main.len()..].as_ptr()
            );
        }

        let options = EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceChunk);
        let plan = chunk_plan(&document, &options);
        assert_eq!(plan.chunk_count(), 1);
        assert_eq!(plan.table_end(), 60);
        assert_eq!(
            plan.primary(),
            WirePrimary {
                chunk_type: ChunkType::IMAGE.raw(),
                hints: PrimaryHints::new(format.to_u8(), width, height, stride),
            }
        );
        let placement = plan.placements().next().unwrap();
        assert_eq!(placement.index, 0);
        assert_eq!(placement.table_entry_offset, CHUNK_FILE_HEADER_LEN);
        assert_eq!(placement.leading_padding, 60..60);
        assert_eq!(placement.chunk_offset, 60);
        assert_eq!(placement.chunk_size, 100);
        assert_eq!(placement.output_range, 60..160);
        assert_eq!(plan.file_size(), 160);
        let PayloadPlan::SegmentedImage(image) = placement.payload else {
            panic!("expected segmented IMAGE payload");
        };
        assert_eq!(image.data_offset, ImageChunkHeader::SIZE as u32);
        assert_eq!(image.data_size, 68);
        assert_eq!(image.payload_size, 100);
        assert_eq!(
            image.image.main.as_ptr(),
            source[FLAT_HEADER_LEN..].as_ptr()
        );
        assert_eq!(
            image.image.extra.unwrap().as_ptr(),
            source[FLAT_HEADER_LEN + main.len()..].as_ptr()
        );
        assert_eq!((placement.chunk_offset + image.data_offset) % 4, 0);
    }

    #[test]
    fn parseable_raw_images_use_the_minimum_absolute_data_alignment() {
        let payloads = [
            image_payload(32),
            image_payload(33),
            image_payload(34),
            image_payload(35),
        ];
        let mut document = Document::new_chunk();
        for payload in &payloads {
            document.push_raw(raw(ChunkType::IMAGE, payload)).unwrap();
        }

        let plan = chunk_plan(&document, &EncodeOptions::new());
        assert_eq!(plan.chunk_count(), 4);
        assert_eq!(plan.table_end(), 108);
        let mut cursor = plan.table_end();
        for (placement, payload) in plan.placements().zip(payloads.iter()) {
            let data_offset = read_u32_le(payload, 16).unwrap();
            let expected_offset = align_relative(cursor, data_offset, 4).unwrap();
            assert_eq!(placement.chunk_type, ChunkType::IMAGE);
            assert_eq!(placement.flags, ChunkFlags::NONE);
            assert_eq!(placement.chunk_size as usize, payload.len());
            assert_eq!(placement.chunk_offset, expected_offset);
            assert_eq!(
                placement.chunk_offset.checked_add(data_offset).unwrap() % 4,
                0
            );
            assert!(placement.chunk_offset - cursor < 4);
            for earlier in cursor..placement.chunk_offset {
                assert_ne!(earlier.checked_add(data_offset).unwrap() % 4, 0);
            }
            assert_eq!(
                placement.leading_padding,
                usize::try_from(cursor).unwrap()..placement.output_range.start
            );
            assert!(ImageView::from_chunk_payload(payload, placement.chunk_offset).is_ok());
            cursor = u32::try_from(placement.output_range.end).unwrap();
        }
        let offsets: Vec<_> = plan
            .placements()
            .skip(1)
            .map(|placement| placement.chunk_offset)
            .collect();
        assert!(offsets.iter().all(|offset| offset % 4 != 0));
        assert_eq!(plan.file_size(), cursor);
    }

    #[test]
    fn raw_image_alignment_covers_every_cursor_and_data_offset_residue() {
        for cursor in 100..104 {
            for data_offset in 32..36 {
                let payload = image_payload(data_offset);
                let constraint =
                    PayloadPlan::Verbatim(&payload).placement_constraint(ChunkType::IMAGE);
                let placed = constraint.place(cursor).unwrap();
                let expected = align_relative(cursor, data_offset, 4).unwrap();

                assert_eq!(placed, expected);
                assert!(placed - cursor < 4);
                assert_eq!(placed.checked_add(data_offset).unwrap() % 4, 0);
                for earlier in cursor..placed {
                    assert_ne!(earlier.checked_add(data_offset).unwrap() % 4, 0);
                }
            }
        }
    }

    #[test]
    fn malformed_image_remains_verbatim_and_following_payloads_realign() {
        let mut malformed = [0u8; 20];
        malformed[16..20].copy_from_slice(&33u32.to_le_bytes());
        let mut document = Document::new_chunk();
        document.push_raw(raw(CUSTOM_A, &[7])).unwrap();
        document
            .push_raw(raw(ChunkType::IMAGE, &malformed))
            .unwrap();
        document.push_raw(raw(CUSTOM_B, &[8, 9, 10])).unwrap();

        let plan = chunk_plan(&document, &EncodeOptions::new());
        let placements: Vec<_> = plan.placements().collect();
        assert_eq!(align_relative(93, 33, 4), Ok(95));
        assert!(ImageView::from_chunk_payload(&malformed, 95).is_err());
        assert_eq!(placements[0].chunk_offset, 92);
        assert_eq!(placements[0].output_range, 92..93);
        assert_eq!(placements[1].chunk_offset, 96);
        assert_eq!(placements[1].chunk_offset % 4, 0);
        assert_eq!(placements[1].output_range, 96..116);
        assert_eq!(placements[2].chunk_offset, 116);
        assert_eq!(placements[2].chunk_offset % 4, 0);
        let PayloadPlan::Verbatim(bytes) = placements[1].payload else {
            panic!("raw IMAGE remains verbatim");
        };
        assert_eq!(bytes, malformed);

        let encoded = document.encode_with(&EncodeOptions::new()).unwrap();
        let reader = Reader::open(&encoded).unwrap();
        let mut chunks = reader.chunks();
        assert_eq!(chunks.next().unwrap().payload(), &[7]);
        assert_eq!(chunks.next().unwrap().payload(), malformed);
        assert_eq!(chunks.next().unwrap().payload(), &[8, 9, 10]);
        assert!(chunks.next().is_none());
    }

    #[test]
    fn primary_states_lower_without_confusing_explicit_zero_with_missing() {
        let image = image_payload(32);
        let mut derived = Document::new_chunk();
        let image_id = derived
            .push_raw(RawChunkInput {
                chunk_type: ChunkType::IMAGE,
                flags: ChunkFlags::NONE,
                payload: PayloadInput::Borrowed(&image),
                policy: RawChunkPolicy::infer(),
            })
            .unwrap();
        derived.set_primary(image_id).unwrap();
        assert_eq!(
            chunk_plan(&derived, &EncodeOptions::new()).primary().hints,
            PrimaryHints::new(ColorFormat::A8.to_u8(), 1, 1, 1)
        );

        let mut explicit_zero = Document::new_chunk();
        let custom_id = explicit_zero.push_raw(raw(CUSTOM_A, b"custom")).unwrap();
        explicit_zero
            .set_primary_with_hints(custom_id, PrimaryHints::ZERO)
            .unwrap();
        assert_eq!(
            chunk_plan(&explicit_zero, &EncodeOptions::new()).primary(),
            WirePrimary {
                chunk_type: CUSTOM_A.raw(),
                hints: PrimaryHints::ZERO,
            }
        );

        let mut known = Document::new_chunk();
        let font = known.push_raw(raw(ChunkType::FONT, b"font")).unwrap();
        known.set_primary(font).unwrap();
        assert_eq!(
            chunk_plan(&known, &EncodeOptions::new()).primary().hints,
            PrimaryHints::new(PRIMARY_FORMAT_NONE, 0, 0, 0)
        );

        let source = encode_chunks(&[(CUSTOM_B.raw(), 0, b"preserved")]);
        let policies = [RawTypePolicy {
            chunk_type: CUSTOM_B,
            policy: relocatable_policy(),
        }];
        let preserved = Document::open_with(
            &source,
            &OpenOptions::new().with_raw_type_policies(&policies),
        )
        .unwrap();
        assert_eq!(
            chunk_plan(&preserved, &EncodeOptions::new()).primary(),
            WirePrimary {
                chunk_type: CUSTOM_B.raw(),
                hints: PrimaryHints::ZERO,
            }
        );
    }

    #[test]
    fn global_blockers_and_primary_missing_precede_capabilities() {
        let mut future_source = encode_chunks(&[(CUSTOM_A.raw(), 0, b"future")]);
        set_future_minor(&mut future_source);
        let future = Document::open(&future_source).unwrap();
        assert_eq!(
            future.encoded_len_with(
                &EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceFlat)
            ),
            Err(EncodeError::FutureSemanticsReadOnly)
        );

        let mut trailing_source = encode_chunks(&[(CUSTOM_A.raw(), 0, b"trailing")]);
        trailing_source.extend_from_slice(b"tail");
        let trailing = Document::open_with(
            &trailing_source,
            &OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve),
        )
        .unwrap();
        assert_eq!(
            trailing.encoded_len_with(&EncodeOptions::new()),
            Err(EncodeError::PreservedTrailingBytesReadOnly)
        );

        let mut missing = Document::new_chunk();
        let id = missing.push_raw(raw(CUSTOM_A, b"missing")).unwrap();
        missing
            .set_primary_with_hints(id, PrimaryHints::ZERO)
            .unwrap();
        let chunks = chunk_set_mut(&mut missing);
        chunks.primary_hints = PrimaryHintState::Missing;
        chunks.chunks[0].capability = super::super::RewriteCapability::PRESERVE_ONLY;
        assert_eq!(
            missing.encoded_len_with(&EncodeOptions::new()),
            Err(EncodeError::PrimaryHintsRequired {
                chunk_type: CUSTOM_A,
            })
        );
    }

    #[test]
    fn capability_errors_follow_table_and_descriptor_priority() {
        let source = encode_chunks(&[(CUSTOM_A.raw(), 0x0002, b"reserved")]);
        let policies = [RawTypePolicy {
            chunk_type: CUSTOM_A,
            policy: relocatable_policy(),
        }];
        let reserved = Document::open_with(
            &source,
            &OpenOptions::new().with_raw_type_policies(&policies),
        )
        .unwrap();
        assert_eq!(
            reserved.encoded_len_with(&EncodeOptions::new()),
            Err(EncodeError::ReservedFlagBits {
                index: 0,
                chunk_type: CUSTOM_A,
                bits: 0x0002,
            })
        );

        let source = encode_chunks(&[
            (CUSTOM_A.raw(), 0, b"first"),
            (CUSTOM_B.raw(), 0, b"second"),
        ]);
        let first_only = [RawTypePolicy {
            chunk_type: CUSTOM_A,
            policy: relocatable_policy(),
        }];
        let relocation = Document::open_with(
            &source,
            &OpenOptions::new().with_raw_type_policies(&first_only),
        )
        .unwrap();
        assert_eq!(
            relocation.encoded_len_with(&EncodeOptions::new()),
            Err(EncodeError::RelocationAssumptionRequired {
                index: 1,
                chunk_type: CUSTOM_B,
            })
        );

        let mut critical = Document::new_chunk();
        critical.push_raw(raw(CUSTOM_A, b"critical")).unwrap();
        let chunks = chunk_set_mut(&mut critical);
        chunks.chunks[0].flags = ChunkFlags::CRITICAL;
        chunks.chunks[0].capability = super::super::RewriteCapability::new(true, false, false);
        assert_eq!(
            critical.encoded_len_with(&EncodeOptions::new()),
            Err(EncodeError::CriticalAssumptionRequired {
                index: 0,
                chunk_type: CUSTOM_A,
            })
        );

        let chunks = chunk_set_mut(&mut critical);
        chunks.chunks[0].flags = ChunkFlags::from_bits_retain(0x0003);
        chunks.chunks[0].capability = super::super::RewriteCapability::PRESERVE_ONLY;
        assert_eq!(
            critical.encoded_len_with(&EncodeOptions::new()),
            Err(EncodeError::ReservedFlagBits {
                index: 0,
                chunk_type: CUSTOM_A,
                bits: 0x0002,
            })
        );
    }

    #[test]
    fn layout_and_wire_arithmetic_boundaries_are_checked() {
        assert_eq!(checked_chunk_count(u16::MAX as usize), Ok(u16::MAX));
        assert_eq!(
            checked_chunk_count(u16::MAX as usize + 1),
            Err(EncodeError::TooManyChunks {
                count: u16::MAX as usize + 1,
            })
        );
        assert_eq!(align_up(0, 4), Ok(0));
        assert_eq!(align_up(u32::MAX - 3, 4), Ok(u32::MAX - 3));
        assert_eq!(align_up(u32::MAX, 4), Err(EncodeError::SizeOverflow));
        assert_eq!(align_relative(0, 33, 4), Ok(3));
        assert_eq!(align_relative(1, 34, 4), Ok(2));
        assert_eq!(
            align_relative(u32::MAX, 32, 4),
            Err(EncodeError::SizeOverflow)
        );
        assert_eq!(
            checked_payload_end(u32::MAX, 1),
            Err(EncodeError::SizeOverflow)
        );
        assert_eq!(checked_payload_end(u32::MAX - 1, 1), Ok(u32::MAX));
        assert_eq!(
            PlacementConstraint::ChunkStartAligned(4).place(u32::MAX),
            Err(EncodeError::SizeOverflow)
        );
        let image = image_payload(32);
        assert_eq!(
            PayloadPlan::Verbatim(&image)
                .placement_constraint(ChunkType::IMAGE)
                .place(u32::MAX),
            Err(EncodeError::SizeOverflow)
        );

        #[cfg(target_pointer_width = "64")]
        assert_eq!(
            checked_payload_len(u32::MAX as usize + 1),
            Err(EncodeError::SizeOverflow)
        );
    }

    #[test]
    fn force_flat_rejects_chunk_without_attempting_demotion() {
        let document = Document::new_chunk();
        assert_eq!(document.layout(), Layout::Chunk);
        assert_eq!(
            document.encoded_len_with(
                &EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceFlat)
            ),
            Err(EncodeError::NotRepresentableAsFlat)
        );
    }

    #[test]
    fn empty_chunk_emits_exact_canonical_header() {
        let document = Document::new_chunk();
        let encoded = document.encode_with(&EncodeOptions::new()).unwrap();

        let mut expected = vec![0; CHUNK_FILE_HEADER_LEN];
        expected[..4].copy_from_slice(b"MIRX");
        expected[4] = VERSION_MAJOR;
        expected[5] = VERSION_MINOR;
        expected[6] = Layout::Chunk.to_u8();
        expected[12..16].copy_from_slice(&(CHUNK_FILE_HEADER_LEN as u32).to_le_bytes());
        expected[16..20].copy_from_slice(&(CHUNK_FILE_HEADER_LEN as u32).to_le_bytes());
        let checksum = crc32(&expected[..40]);
        expected[40..44].copy_from_slice(&checksum.to_le_bytes());

        assert_eq!(encoded, expected);
        assert_eq!(Reader::open(&encoded).unwrap().chunks().len(), 0);
    }

    #[test]
    fn chunk_emission_is_atomic_canonical_and_verbatim() {
        let first_payload = [0x7a];
        let mut checksum_payload = vec![0x10, 0x20, 0x30, 0x40, 0x50];
        let inner_checksum = crc32(&checksum_payload);
        checksum_payload.extend_from_slice(&inner_checksum.to_le_bytes());
        let hints = PrimaryHints::new(0xfe, 17, 9, 23);

        let mut document = Document::new_chunk();
        let primary = document.push_raw(raw(CUSTOM_A, &first_payload)).unwrap();
        document
            .push_raw(RawChunkInput {
                chunk_type: CUSTOM_B,
                flags: ChunkFlags::from_bits_retain(0x0002),
                payload: PayloadInput::Borrowed(&checksum_payload),
                policy: policy(
                    RelocationAssumption::AssumeRelocatable,
                    CriticalAssumption::Infer,
                    ReservedBitsPolicy::Preserve,
                ),
            })
            .unwrap();
        document.set_primary_with_hints(primary, hints).unwrap();

        let options = EncodeOptions::new();
        let needed = document.encoded_len_with(&options).unwrap();
        assert_eq!(needed, 80 + checksum_payload.len());
        let mut out = vec![0xa5; needed + 7];
        assert_eq!(document.encode_into_with(&mut out, &options), Ok(needed));
        assert!(out[needed..].iter().all(|&byte| byte == 0xa5));

        let encoded = &out[..needed];
        assert_eq!(FileHeader::parse(encoded).unwrap().layout, Layout::Chunk);
        assert_eq!(read_u16_le(encoded, 8), Some(2));
        assert_eq!(read_u32_le(encoded, 12), Some(44));
        assert_eq!(read_u32_le(encoded, 16), Some(needed as u32));
        assert_eq!(read_u16_le(encoded, 20), Some(CUSTOM_A.raw()));
        assert_eq!(encoded[22], hints.color_format_raw());
        assert_eq!(read_u32_le(encoded, 24), Some(hints.width()));
        assert_eq!(read_u32_le(encoded, 28), Some(hints.height()));
        assert_eq!(read_u32_le(encoded, 32), Some(hints.stride()));
        assert_eq!(read_u32_le(encoded, 40), Some(crc32(&encoded[..40])));
        assert_eq!(&encoded[10..12], &[0, 0]);
        assert_eq!(encoded[23], 0);
        assert_eq!(&encoded[36..40], &[0, 0, 0, 0]);

        assert_eq!(read_u16_le(encoded, 44), Some(CUSTOM_A.raw()));
        assert_eq!(read_u16_le(encoded, 46), Some(0));
        assert_eq!(read_u32_le(encoded, 48), Some(76));
        assert_eq!(read_u32_le(encoded, 52), Some(1));
        assert_eq!(&encoded[56..60], &[0, 0, 0, 0]);
        assert_eq!(read_u16_le(encoded, 60), Some(CUSTOM_B.raw()));
        assert_eq!(read_u16_le(encoded, 62), Some(0x0002));
        assert_eq!(read_u32_le(encoded, 64), Some(80));
        assert_eq!(
            read_u32_le(encoded, 68),
            Some(u32::try_from(checksum_payload.len()).unwrap())
        );
        assert_eq!(&encoded[72..76], &[0, 0, 0, 0]);
        assert_eq!(&encoded[76..77], &first_payload);
        assert_eq!(&encoded[77..80], &[0, 0, 0]);
        assert_eq!(&encoded[80..], checksum_payload.as_slice());

        let allocated = document.encode_with(&options).unwrap();
        assert_eq!(allocated, encoded);
        let reader = Reader::open(&allocated).unwrap();
        let mut chunks = reader.chunks();
        assert_eq!(chunks.next().unwrap().payload(), first_payload);
        assert_eq!(chunks.next().unwrap().payload(), checksum_payload);
        assert!(chunks.next().is_none());
    }

    #[test]
    fn raw_image_alignment_emits_zero_external_padding_and_verbatim_internal_bytes() {
        let parseable = padded_image_payload(33);
        let mut opaque = image_payload(32);
        opaque[10] = 0x7e;
        let mut frames = vec![0x10, 0x20, 0x30, 0x40, 0x50];
        let frames_checksum = crc32(&frames);
        frames.extend_from_slice(&frames_checksum.to_le_bytes());

        let mut document = Document::new_chunk();
        document
            .push_raw(raw(ChunkType::IMAGE, &parseable))
            .unwrap();
        document.push_raw(raw(ChunkType::IMAGE, &opaque)).unwrap();
        document.push_raw(raw(ChunkType::FRAMES, &frames)).unwrap();

        let options = EncodeOptions::new();
        let needed = document.encoded_len_with(&options).unwrap();
        assert_eq!(needed, 181);
        let mut out = vec![0xa5; needed + 5];
        assert_eq!(document.encode_into_with(&mut out, &options), Ok(needed));
        assert_eq!(&out[needed..], &[0xa5; 5]);

        let encoded = &out[..needed];
        assert_eq!(read_u32_le(encoded, 16), Some(needed as u32));
        assert_eq!(&encoded[10..12], &[0, 0]);
        assert_eq!(encoded[23], 0);
        assert_eq!(&encoded[36..40], &[0, 0, 0, 0]);
        for entry in [44usize, 60, 76] {
            assert_eq!(&encoded[entry + 12..entry + 16], &[0, 0, 0, 0]);
        }

        assert_eq!(read_u32_le(encoded, 48), Some(95));
        assert_eq!(read_u32_le(encoded, 64), Some(136));
        assert_eq!(read_u32_le(encoded, 80), Some(172));
        assert_eq!(&encoded[92..95], &[0, 0, 0]);
        assert_eq!(&encoded[95..136], parseable.as_slice());
        assert_eq!(&encoded[136..169], opaque.as_slice());
        assert_eq!(&encoded[169..172], &[0, 0, 0]);
        assert_eq!(&encoded[172..181], frames.as_slice());
        assert_eq!(read_u32_le(encoded, needed - 4), Some(frames_checksum));
        assert_eq!(95 + read_u32_le(&parseable, 16).unwrap(), 128);
        assert_eq!((95 + read_u32_le(&parseable, 16).unwrap()) % 4, 0);
        assert_ne!(95 % 4, 0);

        let image = ImageView::from_chunk_payload(&encoded[95..136], 95).unwrap();
        assert_eq!(
            image.main(),
            &[0x11, 0xa1, 0xa2, 0xa3, 0x22, 0xb1, 0xb2, 0xb3]
        );

        let reader = Reader::open(encoded).unwrap();
        let mut chunks = reader.chunks();
        let image = chunks.next().unwrap();
        assert_eq!(image.payload_offset(), 95);
        assert_eq!(image.payload(), parseable);
        let opaque_image = chunks.next().unwrap();
        assert_eq!(opaque_image.payload_offset(), 136);
        assert_eq!(opaque_image.payload(), opaque);
        let frames_chunk = chunks.next().unwrap();
        assert_eq!(frames_chunk.payload_offset(), 172);
        assert_eq!(frames_chunk.payload(), frames);
        assert!(chunks.next().is_none());

        let allocated = document.encode_with(&options).unwrap();
        assert_eq!(allocated, encoded);
        assert_eq!(allocated.len(), 172 + frames.len());
    }

    #[test]
    fn zero_length_payloads_align_without_final_padding() {
        let mut document = Document::new_chunk();
        document.push_raw(raw(CUSTOM_A, &[7])).unwrap();
        document.push_raw(raw(ChunkType::IMAGE, &[])).unwrap();
        document.push_raw(raw(ChunkType::FRAMES, &[])).unwrap();
        document.push_raw(raw(CUSTOM_B, &[])).unwrap();

        let plan = chunk_plan(&document, &EncodeOptions::new());
        let placements: Vec<_> = plan.placements().collect();
        assert_eq!(plan.table_end(), 108);
        assert_eq!(placements[0].output_range, 108..109);
        for placement in &placements[1..] {
            assert_eq!(placement.chunk_offset, 112);
            assert_eq!(placement.chunk_offset % 4, 0);
            assert_eq!(placement.output_range, 112..112);
        }
        assert_eq!(plan.file_size(), 112);

        let encoded = document.encode_with(&EncodeOptions::new()).unwrap();
        assert_eq!(encoded.len(), 112);
        assert_eq!(&encoded[109..112], &[0, 0, 0]);
        let reader = Reader::open(&encoded).unwrap();
        let chunks: Vec<_> = reader.chunks().collect();
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].payload(), &[7]);
        assert!(chunks[1..].iter().all(|chunk| chunk.payload().is_empty()));
    }

    #[test]
    fn overlapping_source_ranges_are_flattened_without_changing_visible_bytes() {
        let mut source =
            encode_chunks(&[(CUSTOM_A.raw(), 0, b"abcd"), (CUSTOM_B.raw(), 0, b"wxyz")]);
        let first_offset = read_u32_le(&source, 48).unwrap();
        source[64..68].copy_from_slice(&(first_offset + 2).to_le_bytes());
        source[68..72].copy_from_slice(&2u32.to_le_bytes());
        let policies = [
            RawTypePolicy {
                chunk_type: CUSTOM_A,
                policy: relocatable_policy(),
            },
            RawTypePolicy {
                chunk_type: CUSTOM_B,
                policy: relocatable_policy(),
            },
        ];
        let document = Document::open_with(
            &source,
            &OpenOptions::new().with_raw_type_policies(&policies),
        )
        .unwrap();

        let encoded = document.encode_with(&EncodeOptions::new()).unwrap();
        let reader = Reader::open(&encoded).unwrap();
        let mut chunks = reader.chunks();
        let first = chunks.next().unwrap();
        let second = chunks.next().unwrap();
        assert_eq!(first.payload(), b"abcd");
        assert_eq!(second.payload(), b"cd");
        assert!(
            first.payload_offset() + u32::try_from(first.payload().len()).unwrap()
                <= second.payload_offset()
        );
        assert!(chunks.next().is_none());
    }

    #[test]
    fn encode_into_failures_leave_the_entire_buffer_unchanged() {
        let mut document = Document::new_chunk();
        document.push_raw(raw(CUSTOM_A, b"payload")).unwrap();
        let options = EncodeOptions::new();
        let needed = document.encoded_len_with(&options).unwrap();
        let mut short = vec![0x91; needed - 1];
        let before = short.clone();

        assert_eq!(
            document.encode_into_with(&mut short, &options),
            Err(EncodeError::BufferTooSmall {
                needed,
                available: needed - 1,
            })
        );
        assert_eq!(short, before);

        let force_flat = EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceFlat);
        let mut ample = vec![0x63; needed + 16];
        let before = ample.clone();
        assert_eq!(
            document.encode_into_with(&mut ample, &force_flat),
            Err(EncodeError::NotRepresentableAsFlat)
        );
        assert_eq!(ample, before);

        let mut future_source = encode_chunks(&[(CUSTOM_A.raw(), 0, b"future")]);
        set_future_minor(&mut future_source);
        let future = Document::open(&future_source).unwrap();
        let mut ample = vec![0x44; future_source.len() + 32];
        let before = ample.clone();
        assert_eq!(
            future.encode_into_with(&mut ample, &options),
            Err(EncodeError::FutureSemanticsReadOnly)
        );
        assert_eq!(ample, before);

        let mut trailing_source = encode_chunks(&[(CUSTOM_B.raw(), 0, b"trailing")]);
        trailing_source.extend_from_slice(b"tail");
        let trailing = Document::open_with(
            &trailing_source,
            &OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve),
        )
        .unwrap();
        let mut ample = vec![0x25; trailing_source.len() + 32];
        let before = ample.clone();
        assert_eq!(
            trailing.encode_into_with(&mut ample, &options),
            Err(EncodeError::PreservedTrailingBytesReadOnly)
        );
        assert_eq!(ample, before);
    }

    #[test]
    fn current_flat_public_encoding_is_canonical_and_suffix_safe() {
        let format = ColorFormat::I4;
        let width = 3;
        let height = 2;
        let stride = format.minimum_stride(width).unwrap();
        let main = [1, 2, 3, 4];
        let palette = [0x3c; 64];
        let source = encode_flat(&FlatImageInput {
            width,
            height,
            stride,
            format,
            main: &main,
            extra: Some(&palette),
        });
        let document = Document::open(&source).unwrap();

        let options = EncodeOptions::new();
        let mut short = vec![0x7b; source.len() - 1];
        let before = short.clone();
        assert_eq!(
            document.encode_into_with(&mut short, &options),
            Err(EncodeError::BufferTooSmall {
                needed: source.len(),
                available: source.len() - 1,
            })
        );
        assert_eq!(short, before);

        for policy in [
            LayoutPolicy::PreserveOrPromote,
            LayoutPolicy::SmallestRepresentable,
            LayoutPolicy::ForceFlat,
        ] {
            let options = EncodeOptions::new().with_layout_policy(policy);
            assert_eq!(document.encode_with(&options).unwrap(), source);

            let mut out = vec![0xd2; source.len() + 5];
            assert_eq!(
                document.encode_into_with(&mut out, &options),
                Ok(source.len())
            );
            assert_eq!(&out[..source.len()], source);
            assert_eq!(&out[source.len()..], &[0xd2; 5]);
        }
    }

    #[test]
    fn force_chunk_flat_emits_one_canonical_segmented_image() {
        let format = ColorFormat::RGB565A8;
        let width = 2;
        let height = 1;
        let stride = format.minimum_stride(width).unwrap();
        let main = [0x00, 0xf8, 0xe0, 0x07];
        let alpha = [0x40, 0xc0];
        let source = encode_flat(&FlatImageInput {
            width,
            height,
            stride,
            format,
            main: &main,
            extra: Some(&alpha),
        });
        let document = Document::open(&source).unwrap();
        let options = EncodeOptions::new().with_layout_policy(LayoutPolicy::ForceChunk);
        let encoded = document.encode_with(&options).unwrap();

        assert_eq!(read_u16_le(&encoded, 8), Some(1));
        assert_eq!(read_u32_le(&encoded, 12), Some(44));
        assert_eq!(read_u32_le(&encoded, 16), Some(encoded.len() as u32));
        assert_eq!(read_u16_le(&encoded, 20), Some(ChunkType::IMAGE.raw()));
        assert_eq!(encoded[22], format.to_u8());
        assert_eq!(read_u32_le(&encoded, 24), Some(width));
        assert_eq!(read_u32_le(&encoded, 28), Some(height));
        assert_eq!(read_u32_le(&encoded, 32), Some(stride));
        assert_eq!(read_u32_le(&encoded, 40), Some(crc32(&encoded[..40])));
        assert_eq!(read_u16_le(&encoded, 44), Some(ChunkType::IMAGE.raw()));
        assert_eq!(read_u16_le(&encoded, 46), Some(0));
        assert_eq!(read_u32_le(&encoded, 48), Some(60));
        assert_eq!(read_u32_le(&encoded, 52), Some(38));
        assert_eq!(&encoded[56..60], &[0, 0, 0, 0]);

        let payload = &encoded[60..98];
        assert_eq!(read_u32_le(payload, 0), Some(width));
        assert_eq!(read_u32_le(payload, 4), Some(height));
        assert_eq!(payload[8], format.to_u8());
        assert_eq!(payload[9], 0);
        assert_eq!(&payload[10..12], &[0, 0]);
        assert_eq!(read_u32_le(payload, 12), Some(stride));
        assert_eq!(read_u32_le(payload, 16), Some(32));
        assert_eq!(read_u32_le(payload, 20), Some(6));
        assert_eq!(read_u32_le(payload, 24), Some(2));
        assert_eq!(&payload[28..32], &[0, 0, 0, 0]);
        assert_eq!(&payload[32..36], &main);
        assert_eq!(&payload[36..38], &alpha);

        let image = ImageView::from_chunk_payload(payload, 60).unwrap();
        assert_eq!(image.main(), main);
        assert_eq!(image.extra(), Some(alpha.as_slice()));
    }

    #[test]
    fn finish_clean_borrowed_flat_returns_the_exact_source_slice() {
        let source = flat_source();
        let source_pointer = source.as_ptr();

        let document = Document::open(&source).unwrap();
        assert!(!document.is_dirty());
        let finished = document.finish().unwrap();
        let Cow::Borrowed(bytes) = finished else {
            panic!("unchanged borrowed source must remain borrowed");
        };

        assert_eq!(bytes, source);
        assert_eq!(bytes.as_ptr(), source_pointer);
    }

    #[test]
    fn finish_clean_owned_moves_the_full_source_allocation() {
        let mut encoded = encode_chunks(&[(CUSTOM_A.raw(), 0, b"opaque")]);
        encoded.extend_from_slice(b"preserved-tail");
        let mut source = Vec::with_capacity(encoded.len() + 37);
        source.extend_from_slice(&encoded);
        let source_pointer = source.as_ptr();
        let source_len = source.len();
        let source_capacity = source.capacity();
        let options = OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);

        let document = Document::from_vec_with(source, &options).unwrap();
        assert!(!document.is_dirty());
        assert!(matches!(document.trailing, TrailingState::Preserved));
        let finished = document.finish().unwrap();
        let Cow::Owned(bytes) = finished else {
            panic!("unchanged owned source must remain owned");
        };

        assert_eq!(bytes, encoded);
        assert_eq!(bytes.as_ptr(), source_pointer);
        assert_eq!(bytes.len(), source_len);
        assert_eq!(bytes.capacity(), source_capacity);
        assert!(bytes.ends_with(b"preserved-tail"));
    }

    #[test]
    fn finish_clean_passthrough_ignores_future_and_noncanonical_opaque_state() {
        let mut future_source = encode_chunks(&[(CUSTOM_A.raw(), 0, b"future")]);
        set_future_minor(&mut future_source);
        let future_pointer = future_source.as_ptr();
        let future = Document::open(&future_source).unwrap();
        assert!(!future.is_dirty());
        assert!(future.file_meta().has_future_semantics());
        let finished = future.finish().unwrap();
        let Cow::Borrowed(bytes) = finished else {
            panic!("unchanged future source must remain borrowed");
        };
        assert_eq!(bytes, future_source);
        assert_eq!(bytes.as_ptr(), future_pointer);

        let mut future_flat_source = flat_source();
        set_future_minor(&mut future_flat_source);
        let future_flat_pointer = future_flat_source.as_ptr();
        let future_flat = Document::open(&future_flat_source).unwrap();
        assert!(!future_flat.is_dirty());
        assert!(matches!(future_flat.state, DocumentState::OpaqueFlat(_)));
        let finished = future_flat.finish().unwrap();
        let Cow::Borrowed(bytes) = finished else {
            panic!("unchanged future FLAT source must remain borrowed");
        };
        assert_eq!(bytes, future_flat_source);
        assert_eq!(bytes.as_ptr(), future_flat_pointer);

        let noncanonical = encode_chunks(&[(CUSTOM_A.raw(), 0, b"x"), (CUSTOM_B.raw(), 0, b"yz")]);
        assert_eq!(read_u32_le(&noncanonical, 48), Some(76));
        assert_eq!(read_u32_le(&noncanonical, 64), Some(77));
        let document = Document::open(&noncanonical).unwrap();
        assert!(!document.is_dirty());
        let finished = document.finish().unwrap();
        let Cow::Borrowed(bytes) = finished else {
            panic!("unchanged opaque source must remain borrowed");
        };
        assert_eq!(bytes, noncanonical);
        assert_eq!(read_u32_le(bytes, 64), Some(77));
    }

    #[test]
    fn finish_dirty_and_new_documents_use_default_encoding() {
        let source = encode_chunks(&[(CUSTOM_A.raw(), 0, b"source")]);
        let policies = [RawTypePolicy {
            chunk_type: CUSTOM_A,
            policy: relocatable_policy(),
        }];
        let open_options = OpenOptions::new().with_raw_type_policies(&policies);

        let mut expected_document = Document::open_with(&source, &open_options).unwrap();
        expected_document
            .push_raw(raw(CUSTOM_B, b"inserted"))
            .unwrap();
        let expected = expected_document
            .encode_with(&EncodeOptions::new())
            .unwrap();

        let mut document = Document::open_with(&source, &open_options).unwrap();
        document.push_raw(raw(CUSTOM_B, b"inserted")).unwrap();
        assert!(document.is_dirty());
        let finished = document.finish().unwrap();
        let Cow::Owned(bytes) = finished else {
            panic!("modified document must be rewritten into owned bytes");
        };
        assert_eq!(bytes, expected);

        let flat_source = flat_source();
        let expected_flat = Document::open(&flat_source)
            .unwrap()
            .encode_with(&EncodeOptions::new())
            .unwrap();
        let mut dirty_flat = Document::open(&flat_source).unwrap();
        dirty_flat.dirty = true;
        let finished = dirty_flat.finish().unwrap();
        let Cow::Owned(bytes) = finished else {
            panic!("dirty FLAT document must be rewritten into owned bytes");
        };
        assert_eq!(bytes, expected_flat);
        assert_eq!(Reader::open(&bytes).unwrap().layout(), Layout::Flat);

        let expected_new = Document::new_chunk()
            .encode_with(&EncodeOptions::new())
            .unwrap();
        let mut clean_new = Document::new_chunk();
        clean_new.dirty = false;
        let finished = clean_new.finish().unwrap();
        let Cow::Owned(bytes) = finished else {
            panic!("source-free document must be encoded even when marked clean");
        };
        assert_eq!(bytes, expected_new);
    }

    #[test]
    fn finish_dirty_future_and_trailing_states_keep_default_error_priority() {
        let mut future_source = encode_chunks(&[(CUSTOM_A.raw(), 0, b"future")]);
        set_future_minor(&mut future_source);
        future_source.extend_from_slice(b"tail");
        let preserve_trailing =
            OpenOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
        let mut future = Document::open_with(&future_source, &preserve_trailing).unwrap();
        assert!(matches!(
            future.compatibility,
            Compatibility::FutureReadOnly
        ));
        assert!(matches!(future.trailing, TrailingState::Preserved));
        future.dirty = true;
        assert_eq!(future.finish(), Err(EncodeError::FutureSemanticsReadOnly));

        let mut trailing_source = encode_chunks(&[(CUSTOM_A.raw(), 0, b"trailing")]);
        set_future_minor(&mut trailing_source);
        trailing_source.extend_from_slice(b"tail");
        let options = OpenOptions::new()
            .with_compatibility(CompatibilityPolicy::NormalizeToCurrent)
            .with_trailing_bytes(TrailingBytesPolicy::Preserve);
        let trailing = Document::open_with(&trailing_source, &options).unwrap();
        assert!(trailing.is_dirty());
        assert!(matches!(trailing.compatibility, Compatibility::Current));
        assert!(matches!(trailing.trailing, TrailingState::Preserved));
        assert_eq!(
            trailing.finish(),
            Err(EncodeError::PreservedTrailingBytesReadOnly)
        );
    }

    #[test]
    fn finish_after_discarding_trailing_bytes_rewrites_without_the_tail() {
        let mut source = encode_chunks(&[(CUSTOM_A.raw(), 0, b"source")]);
        let logical_len = source.len();
        source.extend_from_slice(b"tail");
        let policies = [RawTypePolicy {
            chunk_type: CUSTOM_A,
            policy: relocatable_policy(),
        }];
        let options = OpenOptions::new()
            .with_trailing_bytes(TrailingBytesPolicy::Preserve)
            .with_raw_type_policies(&policies);
        let mut document = Document::open_with(&source, &options).unwrap();

        assert!(!document.is_dirty());
        document.discard_trailing_bytes().unwrap();
        assert!(document.is_dirty());
        assert!(matches!(document.trailing, TrailingState::Discarded));
        let finished = document.finish().unwrap();
        let Cow::Owned(bytes) = finished else {
            panic!("discarded trailing bytes require an owned rewrite");
        };

        assert_eq!(bytes.len(), logical_len);
        assert!(!bytes.ends_with(b"tail"));
        let reader = Reader::open(&bytes).unwrap();
        assert_eq!(reader.layout(), Layout::Chunk);
        assert_eq!(reader.logical_len(), bytes.len());
    }

    #[test]
    fn finish_dirty_owned_source_uses_a_new_output_allocation() {
        let encoded = encode_chunks(&[(CUSTOM_A.raw(), 0, b"source")]);
        let mut source = Vec::with_capacity(encoded.len() + 41);
        source.extend_from_slice(&encoded);
        let source_pointer = source.as_ptr();
        let policies = [RawTypePolicy {
            chunk_type: CUSTOM_A,
            policy: relocatable_policy(),
        }];
        let open_options = OpenOptions::new().with_raw_type_policies(&policies);
        let mut document = Document::from_vec_with(source, &open_options).unwrap();
        assert_eq!(document.origin.source().unwrap().as_ptr(), source_pointer);

        document.push_raw(raw(CUSTOM_B, b"inserted")).unwrap();
        let expected = document.encode_with(&EncodeOptions::new()).unwrap();
        let finished = document.finish().unwrap();
        let Cow::Owned(bytes) = finished else {
            panic!("modified owned document must return rewritten owned bytes");
        };

        assert_eq!(bytes, expected);
        assert_ne!(bytes.as_ptr(), source_pointer);
    }
}
