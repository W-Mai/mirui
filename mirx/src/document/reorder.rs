use super::primary::{PrimaryProjection, ensure_primary_projection};
use super::{Document, DocumentState};
use crate::{ChunkId, EditError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RelativePosition {
    Before,
    After,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReorderPlan {
    Noop,
    RotateLeft { start: usize, end: usize },
    RotateRight { start: usize, end: usize },
}

impl ReorderPlan {
    const fn moved_indices(self) -> Option<(usize, usize)> {
        match self {
            Self::Noop => None,
            Self::RotateLeft { start, end } => Some((start, end - 1)),
            Self::RotateRight { start, end } => Some((end - 1, start)),
        }
    }

    fn apply(self, state: &mut DocumentState<'_>) -> bool {
        let Self::Noop = self else {
            let DocumentState::Chunk(chunks) = state else {
                unreachable!("layout was checked while planning chunk reorder");
            };
            match self {
                Self::RotateLeft { start, end } => chunks.chunks[start..end].rotate_left(1),
                Self::RotateRight { start, end } => chunks.chunks[start..end].rotate_right(1),
                Self::Noop => unreachable!("no-op handled before borrowing chunk storage"),
            }
            return true;
        };
        false
    }
}

impl Document<'_> {
    /// Moves `id` immediately before `anchor` without changing either identity.
    pub fn move_before(&mut self, id: ChunkId, anchor: ChunkId) -> Result<(), EditError> {
        self.reorder(id, anchor, RelativePosition::Before)
    }

    /// Moves `id` immediately after `anchor` without changing either identity.
    pub fn move_after(&mut self, id: ChunkId, anchor: ChunkId) -> Result<(), EditError> {
        self.reorder(id, anchor, RelativePosition::After)
    }

    fn reorder(
        &mut self,
        id: ChunkId,
        anchor: ChunkId,
        position: RelativePosition,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let plan = plan_reorder(&self.state, id, anchor, position)?;
        if let Some((source, destination)) = plan.moved_indices() {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout checked before projecting chunk reorder");
            };
            ensure_primary_projection(
                chunks,
                PrimaryProjection::Move {
                    source,
                    destination,
                },
            )?;
        }
        if plan.apply(&mut self.state) {
            self.dirty = true;
        }
        Ok(())
    }
}

fn plan_reorder(
    state: &DocumentState<'_>,
    id: ChunkId,
    anchor: ChunkId,
    position: RelativePosition,
) -> Result<ReorderPlan, EditError> {
    let DocumentState::Chunk(chunks) = state else {
        return Err(EditError::ChunkLayoutRequired);
    };

    // Resolve both identities before accepting a self move as a no-op.
    let source_index = chunks
        .chunks
        .iter()
        .position(|node| node.id == id)
        .ok_or(EditError::InvalidChunkId)?;
    let anchor_index = chunks
        .chunks
        .iter()
        .position(|node| node.id == anchor)
        .ok_or(EditError::InvalidChunkId)?;

    if source_index == anchor_index {
        return Ok(ReorderPlan::Noop);
    }

    let plan = match position {
        RelativePosition::Before if source_index + 1 == anchor_index => ReorderPlan::Noop,
        RelativePosition::Before if source_index < anchor_index => ReorderPlan::RotateLeft {
            start: source_index,
            end: anchor_index,
        },
        RelativePosition::Before => ReorderPlan::RotateRight {
            start: anchor_index,
            end: source_index + 1,
        },
        RelativePosition::After if anchor_index + 1 == source_index => ReorderPlan::Noop,
        RelativePosition::After if source_index < anchor_index => ReorderPlan::RotateLeft {
            start: source_index,
            end: anchor_index + 1,
        },
        RelativePosition::After => ReorderPlan::RotateRight {
            start: anchor_index + 1,
            end: source_index + 1,
        },
    };
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::document::source::SourceRange;
    use crate::{
        ChunkFlags, ChunkType, ColorFormat, CriticalAssumption, FlatImageInput, PayloadInput,
        RawChunkInput, RawChunkPolicy, RelocationAssumption, ReservedBitsPolicy, encode_chunks,
        encode_flat,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct NodeSnapshot {
        id: ChunkId,
        chunk_type: ChunkType,
        flags: ChunkFlags,
        capability: super::super::RewriteCapability,
        storage_kind: u8,
        source_range: Option<SourceRange>,
        payload_pointer: usize,
        payload_len: usize,
        owned_capacity: Option<usize>,
    }

    #[derive(Debug, Eq, PartialEq)]
    struct DocumentSnapshot {
        nodes: Vec<NodeSnapshot>,
        vector_pointer: usize,
        vector_capacity: usize,
        primary: Option<ChunkId>,
        next_id: u32,
        dirty: bool,
    }

    const fn assumed_policy() -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
            reserved_flag_bits: ReservedBitsPolicy::Reject,
        }
    }

    fn raw<'a>(
        chunk_type: ChunkType,
        flags: ChunkFlags,
        payload: PayloadInput<'a>,
    ) -> RawChunkInput<'a> {
        RawChunkInput {
            chunk_type,
            flags,
            payload,
            policy: assumed_policy(),
        }
    }

    fn four_chunk_document() -> Document<'static> {
        let mut document = Document::new_chunk();
        for (chunk_type, payload) in [
            (ChunkType::META, b"zero".as_slice()),
            (ChunkType::FONT, b"one".as_slice()),
            (ChunkType::PALETTE, b"two".as_slice()),
            (ChunkType::new(0xbeef).unwrap(), b"three".as_slice()),
        ] {
            document
                .push_raw(raw(
                    chunk_type,
                    ChunkFlags::NONE,
                    PayloadInput::Borrowed(payload),
                ))
                .unwrap();
        }
        document.dirty = false;
        document
    }

    fn ids(document: &Document<'_>) -> Vec<ChunkId> {
        document.chunks().map(|chunk| chunk.id()).collect()
    }

    fn expected_ids(indices: &[u32]) -> Vec<ChunkId> {
        indices
            .iter()
            .copied()
            .map(ChunkId::from_session_counter)
            .collect()
    }

    fn chunk_nodes<'document, 'source>(
        document: &'document Document<'source>,
    ) -> &'document [super::super::ChunkNode<'source>] {
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("expected CHUNK document");
        };
        chunks.chunks.as_slice()
    }

    fn node_snapshot(document: &Document<'_>, index: usize) -> NodeSnapshot {
        let node = &chunk_nodes(document)[index];
        let (storage_kind, source_range, bytes, owned_capacity) = match &node.payload {
            super::super::PayloadStorage::SourceRange(range) => (
                0,
                Some(*range),
                document.origin.resolve(*range).unwrap(),
                None,
            ),
            super::super::PayloadStorage::Borrowed(bytes) => (1, None, *bytes, None),
            super::super::PayloadStorage::Owned(bytes) => {
                (2, None, bytes.as_slice(), Some(bytes.capacity()))
            }
            super::super::PayloadStorage::PromotedFlat => (3, None, &[] as &[u8], None),
        };
        NodeSnapshot {
            id: node.id,
            chunk_type: node.chunk_type,
            flags: node.flags,
            capability: node.capability,
            storage_kind,
            source_range,
            payload_pointer: bytes.as_ptr() as usize,
            payload_len: bytes.len(),
            owned_capacity,
        }
    }

    fn snapshot(document: &Document<'_>) -> DocumentSnapshot {
        let nodes = chunk_nodes(document);
        DocumentSnapshot {
            nodes: (0..nodes.len())
                .map(|index| node_snapshot(document, index))
                .collect(),
            vector_pointer: nodes.as_ptr() as usize,
            vector_capacity: match &document.state {
                DocumentState::Chunk(chunks) => chunks.chunks.capacity(),
                _ => unreachable!(),
            },
            primary: document.primary(),
            next_id: document.next_id,
            dirty: document.dirty,
        }
    }

    #[test]
    fn move_before_and_after_cover_both_directions_and_endpoints() {
        let id = ChunkId::from_session_counter;

        let mut before_forward = four_chunk_document();
        before_forward.move_before(id(0), id(3)).unwrap();
        assert_eq!(ids(&before_forward), expected_ids(&[1, 2, 0, 3]));
        assert!(before_forward.is_dirty());
        assert_eq!(before_forward.next_id, 4);

        let mut before_backward = four_chunk_document();
        before_backward.move_before(id(3), id(0)).unwrap();
        assert_eq!(ids(&before_backward), expected_ids(&[3, 0, 1, 2]));
        assert!(before_backward.is_dirty());
        assert_eq!(before_backward.next_id, 4);

        let mut after_forward = four_chunk_document();
        after_forward.move_after(id(0), id(3)).unwrap();
        assert_eq!(ids(&after_forward), expected_ids(&[1, 2, 3, 0]));
        assert!(after_forward.is_dirty());
        assert_eq!(after_forward.next_id, 4);

        let mut after_backward = four_chunk_document();
        after_backward.move_after(id(3), id(0)).unwrap();
        assert_eq!(ids(&after_backward), expected_ids(&[0, 3, 1, 2]));
        assert!(after_backward.is_dirty());
        assert_eq!(after_backward.next_id, 4);
    }

    #[test]
    fn self_and_already_adjacent_moves_are_clean_noops() {
        let mut document = four_chunk_document();
        let original = snapshot(&document);
        let id = ChunkId::from_session_counter;

        for result in [
            document.move_before(id(1), id(1)),
            document.move_after(id(1), id(1)),
            document.move_before(id(1), id(2)),
            document.move_after(id(2), id(1)),
        ] {
            assert_eq!(result, Ok(()));
        }
        assert_eq!(snapshot(&document), original);
    }

    #[test]
    fn reorder_preserves_mixed_storage_allocations_descriptors_and_primary_identity() {
        let mut source = encode_chunks(&[
            (ChunkType::META.raw(), 0, b"source-primary"),
            (ChunkType::FONT.raw(), 0, b"source-font"),
        ]);
        source[20..22].copy_from_slice(&ChunkType::META.raw().to_le_bytes());
        let checksum = crate::crc32(&source[..40]);
        source[40..44].copy_from_slice(&checksum.to_le_bytes());

        let borrowed = [3, 1, 4, 1, 5];
        let owned = Vec::from([9, 2, 6, 5, 3, 5]);
        let owned_pointer = owned.as_ptr() as usize;
        let owned_capacity = owned.capacity();
        let mut document = Document::from_vec(source).unwrap();
        let source_ids = ids(&document);
        let borrowed_id = document
            .push_raw(raw(
                ChunkType::PALETTE,
                ChunkFlags::CRITICAL,
                PayloadInput::Borrowed(&borrowed),
            ))
            .unwrap();
        let owned_id = document
            .push_raw(raw(
                ChunkType::new(0xbeef).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Owned(owned),
            ))
            .unwrap();

        let before = snapshot(&document);
        let node_snapshots = before.nodes.clone();
        let primary = document.primary().unwrap();
        assert_eq!(primary, source_ids[0]);
        assert_eq!(
            node_snapshots
                .iter()
                .find(|node| node.id == owned_id)
                .unwrap()
                .payload_pointer,
            owned_pointer
        );
        assert_eq!(
            node_snapshots
                .iter()
                .find(|node| node.id == owned_id)
                .unwrap()
                .owned_capacity,
            Some(owned_capacity)
        );

        document.next_id = u32::MAX;
        document.dirty = false;
        document.move_after(primary, owned_id).unwrap();

        assert_eq!(
            ids(&document),
            [source_ids[1], borrowed_id, owned_id, primary]
        );
        assert_eq!(document.primary(), Some(primary));
        assert_eq!(document.get(primary).unwrap().chunk_type(), ChunkType::META);
        assert_eq!(document.next_id, u32::MAX);
        assert!(document.is_dirty());
        let after = snapshot(&document);
        assert_eq!(after.vector_pointer, before.vector_pointer);
        assert_eq!(after.vector_capacity, before.vector_capacity);

        for expected in node_snapshots {
            let actual_index = chunk_nodes(&document)
                .iter()
                .position(|node| node.id == expected.id)
                .unwrap();
            assert_eq!(node_snapshot(&document, actual_index), expected);
        }
    }

    #[test]
    fn reordering_with_id_holes_keeps_the_counter_and_remaining_identities() {
        let mut document = four_chunk_document();
        let id = ChunkId::from_session_counter;
        document.remove(id(1)).unwrap();
        document.dirty = false;
        let next_id = document.next_id;

        document.move_before(id(3), id(0)).unwrap();

        assert_eq!(ids(&document), expected_ids(&[3, 0, 2]));
        assert_eq!(document.next_id, next_id);
        assert!(document.get(id(1)).is_none());
        assert!(document.is_dirty());
    }

    #[test]
    fn invalid_ids_and_flat_layout_leave_the_document_unchanged() {
        let mut document = four_chunk_document();
        let id = ChunkId::from_session_counter;
        let invalid = id(99);
        let original = snapshot(&document);

        for result in [
            document.move_before(invalid, id(0)),
            document.move_after(id(0), invalid),
            document.move_before(invalid, invalid),
        ] {
            assert_eq!(result, Err(EditError::InvalidChunkId));
            assert_eq!(snapshot(&document), original);
        }

        let flat_source = encode_flat(&FlatImageInput {
            width: 1,
            height: 1,
            stride: 1,
            format: ColorFormat::A8,
            main: &[7],
            extra: None,
        });
        let mut flat = Document::open(&flat_source).unwrap();
        assert_eq!(
            flat.move_before(invalid, invalid),
            Err(EditError::ChunkLayoutRequired)
        );
        assert_eq!(
            flat.move_after(invalid, invalid),
            Err(EditError::ChunkLayoutRequired)
        );
        assert!(!flat.is_dirty());
        assert_eq!(flat.flat_image().unwrap().main(), [7]);
    }
}
