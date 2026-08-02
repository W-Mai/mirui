#![allow(
    dead_code,
    reason = "layout plans are replayed by the document emitter"
)]

use core::ops::Range;

use super::{
    ChunkNode, ChunkSet, Compatibility, Document, DocumentState, EncodeOptions, FlatRecord,
    LayoutPolicy, PayloadStorage, PrimaryHintState, TrailingState,
};
use crate::{
    CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, ChunkFlags, ChunkType, ColorFormat, EncodeError,
    FLAT_HEADER_LEN, ImageChunkHeader, PrimaryHints,
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

impl PayloadPlan<'_> {
    fn encoded_len(self) -> Result<u32, EncodeError> {
        match self {
            Self::Verbatim(bytes) => checked_payload_len(bytes.len()),
            Self::SegmentedImage(image) => Ok(image.payload_size),
        }
    }

    fn placement_constraint(self, _chunk_type: ChunkType) -> PlacementConstraint {
        PlacementConstraint::ChunkStartAligned(CONTAINER_ALIGNMENT)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PlacementConstraint {
    ChunkStartAligned(u32),
}

impl PlacementConstraint {
    fn place(self, cursor: u32) -> Result<u32, EncodeError> {
        match self {
            Self::ChunkStartAligned(alignment) => align_up(cursor, alignment),
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
    /// Returns the exact output length after checking the selected layout.
    pub fn encoded_len_with(&self, options: &EncodeOptions) -> Result<usize, EncodeError> {
        Ok(self.layout_plan_with(options)?.output_len())
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

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;
    use core::mem::{needs_drop, size_of};

    use super::*;
    use crate::document::{
        CriticalAssumption, OpenOptions, PayloadInput, RawChunkInput, RawChunkPolicy,
        RawTypePolicy, RelocationAssumption, ReservedBitsPolicy,
    };
    use crate::{
        FlatImageInput, Layout, PRIMARY_FORMAT_NONE, TrailingBytesPolicy, VERSION_MINOR, crc32,
        encode_chunks, encode_flat,
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

    fn set_future_minor(source: &mut [u8]) {
        source[5] = VERSION_MINOR + 1;
        let checksum = crc32(&source[..40]);
        source[40..44].copy_from_slice(&checksum.to_le_bytes());
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
    fn verbatim_payloads_use_the_extensible_start_alignment_constraint() {
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
            assert_eq!(placement.chunk_type, ChunkType::IMAGE);
            assert_eq!(placement.flags, ChunkFlags::NONE);
            assert_eq!(placement.chunk_size as usize, payload.len());
            assert_eq!(placement.chunk_offset % 4, 0);
            assert!(placement.chunk_offset >= cursor);
            assert!(placement.chunk_offset - cursor < 4);
            assert_eq!(
                placement.leading_padding,
                usize::try_from(cursor).unwrap()..placement.output_range.start
            );
            cursor = u32::try_from(placement.output_range.end).unwrap();
        }
        assert_eq!(plan.file_size(), cursor);
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
        assert_eq!(
            checked_payload_end(u32::MAX, 1),
            Err(EncodeError::SizeOverflow)
        );
        assert_eq!(checked_payload_end(u32::MAX - 1, 1), Ok(u32::MAX));
        assert_eq!(
            PlacementConstraint::ChunkStartAligned(4).place(u32::MAX),
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
}
