use super::descriptor::descriptor_payload;
use super::{ChunkNode, ChunkSet, Document, DocumentState, PrimaryHintState};
use crate::{ChunkId, ChunkType, EditError, ImageView, PRIMARY_FORMAT_NONE, PrimaryHints};

const KNOWN_NON_IMAGE_HINTS: PrimaryHints = PrimaryHints::new(PRIMARY_FORMAT_NONE, 0, 0, 0);

const fn is_known_non_image(chunk_type: ChunkType) -> bool {
    matches!(
        chunk_type,
        ChunkType::FONT | ChunkType::VECTOR | ChunkType::META | ChunkType::PALETTE
    )
}

pub(super) const fn open_primary_hint_state(
    chunk_type: ChunkType,
    known_contract: bool,
    preserve_opaque: bool,
    wire_hints: PrimaryHints,
) -> PrimaryHintState {
    if matches!(chunk_type, ChunkType::IMAGE) && known_contract {
        PrimaryHintState::Derived
    } else if is_known_non_image(chunk_type) {
        known_non_image_hint_state(wire_hints)
    } else if preserve_opaque {
        PrimaryHintState::PreservedOpaque(wire_hints)
    } else {
        PrimaryHintState::Missing
    }
}

const fn valid_known_non_image_hints(hints: PrimaryHints) -> bool {
    hints.color_format_raw() == PRIMARY_FORMAT_NONE
        && hints.stride() == 0
        && ((hints.width() == 0 && hints.height() == 0)
            || (hints.width() != 0 && hints.height() != 0))
}

const fn known_non_image_hint_state(hints: PrimaryHints) -> PrimaryHintState {
    if valid_known_non_image_hints(hints) && hints.width() != 0 {
        PrimaryHintState::Explicit(hints)
    } else {
        PrimaryHintState::KnownNonImageDefault
    }
}

pub(super) fn changed_primary_hint_state(
    chunk_type: ChunkType,
    payload: &[u8],
    payload_offset: u32,
) -> PrimaryHintState {
    if matches!(chunk_type, ChunkType::IMAGE)
        && ImageView::from_chunk_payload(payload, payload_offset).is_ok()
    {
        PrimaryHintState::Derived
    } else if is_known_non_image(chunk_type) {
        PrimaryHintState::KnownNonImageDefault
    } else {
        PrimaryHintState::Missing
    }
}

fn selected_primary_hint_state(
    chunk_type: ChunkType,
    payload: &[u8],
    payload_offset: u32,
) -> Result<PrimaryHintState, EditError> {
    let state = changed_primary_hint_state(chunk_type, payload, payload_offset);
    if matches!(state, PrimaryHintState::Missing) {
        Err(EditError::PrimaryHintsRequired { chunk_type })
    } else {
        Ok(state)
    }
}

fn explicit_primary_hint_state(
    chunk_type: ChunkType,
    payload: &[u8],
    payload_offset: u32,
    hints: PrimaryHints,
) -> Result<PrimaryHintState, EditError> {
    if matches!(chunk_type, ChunkType::IMAGE) {
        return match ImageView::from_chunk_payload(payload, payload_offset) {
            Ok(image) if image_hints(image) == hints => Ok(PrimaryHintState::Derived),
            Ok(_) => Err(EditError::InvalidPrimaryHints { chunk_type }),
            Err(_) => Ok(PrimaryHintState::Explicit(hints)),
        };
    }
    if is_known_non_image(chunk_type) {
        return if valid_known_non_image_hints(hints) {
            Ok(known_non_image_hint_state(hints))
        } else {
            Err(EditError::InvalidPrimaryHints { chunk_type })
        };
    }
    Ok(PrimaryHintState::Explicit(hints))
}

fn image_hints(image: ImageView<'_>) -> PrimaryHints {
    PrimaryHints::new(
        image.format().to_u8(),
        image.width(),
        image.height(),
        image.stride(),
    )
}

/// A structural edit projected onto table order without changing storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PrimaryProjection {
    Insert { index: usize, chunk_type: ChunkType },
    Move { source: usize, destination: usize },
    SetType { index: usize, chunk_type: ChunkType },
}

#[derive(Clone, Copy)]
struct ProjectedNode {
    id: Option<ChunkId>,
    chunk_type: ChunkType,
}

impl PrimaryProjection {
    fn output_len(self, input_len: usize) -> usize {
        match self {
            Self::Insert { .. } => input_len
                .checked_add(1)
                .expect("a live Vec cannot contain usize::MAX nodes"),
            Self::Move { .. } | Self::SetType { .. } => input_len,
        }
    }

    fn node_at(self, nodes: &[ChunkNode<'_>], output: usize) -> ProjectedNode {
        match self {
            Self::Insert { index, chunk_type } if output == index => ProjectedNode {
                id: None,
                chunk_type,
            },
            Self::Insert { index, .. } => {
                projected_node(&nodes[if output < index { output } else { output - 1 }])
            }
            Self::Move {
                source,
                destination,
            } if output == destination => projected_node(&nodes[source]),
            Self::Move {
                source,
                destination,
            } if source < destination && (source..destination).contains(&output) => {
                projected_node(&nodes[output + 1])
            }
            Self::Move {
                source,
                destination,
            } if destination < source && (destination + 1..=source).contains(&output) => {
                projected_node(&nodes[output - 1])
            }
            Self::Move { .. } => projected_node(&nodes[output]),
            Self::SetType { index, chunk_type } if output == index => ProjectedNode {
                id: Some(nodes[output].id),
                chunk_type,
            },
            Self::SetType { .. } => projected_node(&nodes[output]),
        }
    }
}

fn projected_node(node: &ChunkNode<'_>) -> ProjectedNode {
    ProjectedNode {
        id: Some(node.id),
        chunk_type: node.chunk_type,
    }
}

/// Rejects a projection whose first node of the primary type is not primary.
pub(super) fn ensure_primary_projection(
    chunks: &ChunkSet<'_>,
    projection: PrimaryProjection,
) -> Result<(), EditError> {
    let Some(primary) = chunks.primary else {
        return Ok(());
    };
    let primary_index = chunks
        .chunks
        .iter()
        .position(|node| node.id == primary)
        .expect("document primary must identify a live node");
    let primary_type = match projection {
        PrimaryProjection::SetType { index, chunk_type } if index == primary_index => chunk_type,
        _ => chunks.chunks[primary_index].chunk_type,
    };

    for output in 0..projection.output_len(chunks.chunks.len()) {
        let node = projection.node_at(&chunks.chunks, output);
        if node.chunk_type == primary_type {
            return if node.id == Some(primary) {
                Ok(())
            } else {
                Err(EditError::WouldShadowPrimary)
            };
        }
    }
    unreachable!("a structural projection cannot remove the primary node")
}

impl Document<'_> {
    /// Returns the effective primary display hints.
    ///
    /// Valid IMAGE hints are derived from the validated payload. Known
    /// non-image primaries use [`PRIMARY_FORMAT_NONE`], zero stride, and either
    /// explicit suggested dimensions or zero geometry. Future opaque FLAT
    /// documents expose their preserved raw header hints without interpreting
    /// the format. Documents without a primary and primaries whose hints are
    /// missing return [`PrimaryHints::ZERO`].
    pub fn primary_hints(&self) -> PrimaryHints {
        let DocumentState::Chunk(chunks) = &self.state else {
            return match &self.state {
                DocumentState::Flat(record) => PrimaryHints::new(
                    record.image.format.to_u8(),
                    record.image.width,
                    record.image.height,
                    record.image.stride,
                ),
                DocumentState::OpaqueFlat(hints) => *hints,
                DocumentState::Chunk(_) => unreachable!("CHUNK handled before FLAT hints"),
            };
        };
        let Some(primary) = chunks.primary else {
            return PrimaryHints::ZERO;
        };
        match chunks.primary_hints {
            PrimaryHintState::Derived => {
                let node = chunks
                    .chunks
                    .iter()
                    .find(|node| node.id == primary)
                    .expect("document primary must identify a live node");
                let (payload, payload_offset) = descriptor_payload(self, node);
                let image = ImageView::from_chunk_payload(payload, payload_offset)
                    .expect("derived primary hints require a validated IMAGE payload");
                image_hints(image)
            }
            PrimaryHintState::Explicit(hints) | PrimaryHintState::PreservedOpaque(hints) => hints,
            PrimaryHintState::KnownNonImageDefault => KNOWN_NON_IMAGE_HINTS,
            PrimaryHintState::Missing => PrimaryHints::ZERO,
        }
    }

    /// Selects a primary chunk by stable identity.
    ///
    /// When an earlier same-typed node exists, the selected node is moved to
    /// that type's first table position without allocating. Other nodes retain
    /// their relative order. Opaque IMAGE, FRAMES, and custom payloads require
    /// [`Document::set_primary_with_hints`].
    pub fn set_primary(&mut self, id: ChunkId) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let (selected, first_of_type, primary_hints) = {
            let DocumentState::Chunk(chunks) = &self.state else {
                return Err(EditError::ChunkLayoutRequired);
            };
            let selected = chunks
                .chunks
                .iter()
                .position(|node| node.id == id)
                .ok_or(EditError::InvalidChunkId)?;
            if chunks.primary == Some(id)
                && !matches!(chunks.primary_hints, PrimaryHintState::Missing)
            {
                return Ok(());
            }
            let chunk_type = chunks.chunks[selected].chunk_type;
            let (payload, payload_offset) = descriptor_payload(self, &chunks.chunks[selected]);
            let primary_hints = selected_primary_hint_state(chunk_type, payload, payload_offset)?;
            let first_of_type = chunks
                .chunks
                .iter()
                .position(|node| node.chunk_type == chunk_type)
                .expect("the selected node is a matching type occurrence");
            (selected, first_of_type, primary_hints)
        };

        let DocumentState::Chunk(chunks) = &mut self.state else {
            unreachable!("layout was checked before selecting primary");
        };
        if first_of_type != selected {
            chunks.chunks[first_of_type..=selected].rotate_right(1);
        }
        chunks.primary = Some(id);
        chunks.primary_hints = primary_hints;
        self.dirty = true;
        Ok(())
    }

    /// Selects a primary chunk with caller-supplied display hints.
    ///
    /// This is the explicit path for payload contracts whose hints cannot be
    /// derived by this crate. Valid IMAGE payloads accept only their derived
    /// hints. Known non-image payloads require [`PRIMARY_FORMAT_NONE`], zero
    /// stride, and either zero geometry or nonzero suggested dimensions. The
    /// selected node is moved to the first table position of its type without
    /// allocating.
    pub fn set_primary_with_hints(
        &mut self,
        id: ChunkId,
        hints: PrimaryHints,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let (selected, first_of_type, primary_hints) = {
            let DocumentState::Chunk(chunks) = &self.state else {
                return Err(EditError::ChunkLayoutRequired);
            };
            let selected = chunks
                .chunks
                .iter()
                .position(|node| node.id == id)
                .ok_or(EditError::InvalidChunkId)?;
            let node = &chunks.chunks[selected];
            let (payload, payload_offset) = descriptor_payload(self, node);
            let primary_hints =
                explicit_primary_hint_state(node.chunk_type, payload, payload_offset, hints)?;
            let exact_preserved = matches!(
                chunks.primary_hints,
                PrimaryHintState::PreservedOpaque(preserved) if preserved == hints
            );
            if chunks.primary == Some(id)
                && (chunks.primary_hints == primary_hints || exact_preserved)
            {
                return Ok(());
            }
            let chunk_type = node.chunk_type;
            let first_of_type = chunks
                .chunks
                .iter()
                .position(|node| node.chunk_type == chunk_type)
                .expect("the selected node is a matching type occurrence");
            (selected, first_of_type, primary_hints)
        };

        let DocumentState::Chunk(chunks) = &mut self.state else {
            unreachable!("layout was checked before selecting explicit primary hints");
        };
        if first_of_type != selected {
            chunks.chunks[first_of_type..=selected].rotate_right(1);
        }
        chunks.primary = Some(id);
        chunks.primary_hints = primary_hints;
        self.dirty = true;
        Ok(())
    }

    /// Clears the selected primary chunk without changing table order.
    pub fn clear_primary(&mut self) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let DocumentState::Chunk(chunks) = &mut self.state else {
            return Err(EditError::ChunkLayoutRequired);
        };
        if chunks.primary.take().is_some() {
            chunks.primary_hints = PrimaryHintState::Missing;
            self.dirty = true;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::super::{
        Compatibility, FileMeta, Origin, PayloadInput, PayloadStorage, RawChunkInput,
        RawChunkPolicy, RelocationAssumption, RewriteCapability, TrailingState,
    };
    use super::*;
    use crate::{
        ChunkFlags, CriticalAssumption, FlatImageInput, ReservedBitsPolicy, crc32, encode_chunks,
        encode_flat,
    };

    const TYPE_A: ChunkType = match ChunkType::new(0xa001) {
        Some(chunk_type) => chunk_type,
        None => panic!("nonzero chunk type"),
    };
    const TYPE_B: ChunkType = match ChunkType::new(0xb001) {
        Some(chunk_type) => chunk_type,
        None => panic!("nonzero chunk type"),
    };
    const TYPE_C: ChunkType = match ChunkType::new(0xc001) {
        Some(chunk_type) => chunk_type,
        None => panic!("nonzero chunk type"),
    };
    const TYPE_D: ChunkType = match ChunkType::new(0xd001) {
        Some(chunk_type) => chunk_type,
        None => panic!("nonzero chunk type"),
    };
    const EXPLICIT_HINTS: PrimaryHints = PrimaryHints::new(0xa5, 13, 21, 55);

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct NodeSnapshot {
        id: ChunkId,
        chunk_type: ChunkType,
        flags: ChunkFlags,
        capability: RewriteCapability,
        storage_kind: u8,
        payload_pointer: usize,
        payload_len: usize,
        owned_capacity: Option<usize>,
    }

    #[derive(Debug, Eq, PartialEq)]
    struct DocumentSnapshot {
        logical_len: usize,
        file: FileMeta,
        compatibility: Compatibility,
        trailing: TrailingState,
        dirty: bool,
        next_id: u32,
        primary: Option<ChunkId>,
        primary_hints: PrimaryHintState,
        origin_kind: u8,
        origin_pointer: usize,
        origin_len: usize,
        origin_capacity: Option<usize>,
        vector_pointer: usize,
        vector_capacity: usize,
        nodes: Vec<NodeSnapshot>,
    }

    const fn explicit_policy() -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
            reserved_flag_bits: ReservedBitsPolicy::Reject,
        }
    }

    fn raw(
        chunk_type: ChunkType,
        payload: &'static [u8],
        policy: RawChunkPolicy,
    ) -> RawChunkInput<'static> {
        RawChunkInput {
            chunk_type,
            flags: ChunkFlags::NONE,
            payload: PayloadInput::Borrowed(payload),
            policy,
        }
    }

    fn set_wire_primary(source: &mut [u8], chunk_type: ChunkType) {
        source[20..22].copy_from_slice(&chunk_type.raw().to_le_bytes());
        let checksum = crc32(&source[..40]);
        source[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    fn clear_wire_primary(source: &mut [u8]) {
        source[20..22].copy_from_slice(&0u16.to_le_bytes());
        let checksum = crc32(&source[..40]);
        source[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    fn id(index: u32) -> ChunkId {
        ChunkId::from_session_counter(index)
    }

    fn ids(document: &Document<'_>) -> Vec<ChunkId> {
        document.chunks().map(|chunk| chunk.id()).collect()
    }

    fn types(document: &Document<'_>) -> Vec<ChunkType> {
        document.chunks().map(|chunk| chunk.chunk_type()).collect()
    }

    fn snapshot(document: &Document<'_>) -> DocumentSnapshot {
        let (origin_kind, origin_pointer, origin_len, origin_capacity) = match &document.origin {
            Origin::New => (0, 0, 0, None),
            Origin::Borrowed(bytes) => (1, bytes.as_ptr() as usize, bytes.len(), None),
            Origin::Owned(bytes) => (
                2,
                bytes.as_ptr() as usize,
                bytes.len(),
                Some(bytes.capacity()),
            ),
        };
        let (primary, primary_hints, vector_pointer, vector_capacity, nodes) = match &document.state
        {
            DocumentState::Chunk(chunks) => {
                let nodes = chunks
                    .chunks
                    .iter()
                    .map(|node| {
                        let (storage_kind, payload, owned_capacity) = match &node.payload {
                            PayloadStorage::SourceRange(range) => (
                                0,
                                document
                                    .origin
                                    .resolve(*range)
                                    .expect("validated source range"),
                                None,
                            ),
                            PayloadStorage::Borrowed(bytes) => (1, *bytes, None),
                            PayloadStorage::Owned(bytes) => {
                                (2, bytes.as_slice(), Some(bytes.capacity()))
                            }
                        };
                        NodeSnapshot {
                            id: node.id,
                            chunk_type: node.chunk_type,
                            flags: node.flags,
                            capability: node.capability,
                            storage_kind,
                            payload_pointer: payload.as_ptr() as usize,
                            payload_len: payload.len(),
                            owned_capacity,
                        }
                    })
                    .collect();
                (
                    chunks.primary,
                    chunks.primary_hints,
                    chunks.chunks.as_ptr() as usize,
                    chunks.chunks.capacity(),
                    nodes,
                )
            }
            DocumentState::Flat(_) | DocumentState::OpaqueFlat(_) => {
                (None, PrimaryHintState::Missing, 0, 0, Vec::new())
            }
        };
        DocumentSnapshot {
            logical_len: document.logical_len,
            file: document.file,
            compatibility: document.compatibility,
            trailing: document.trailing,
            dirty: document.dirty,
            next_id: document.next_id,
            primary,
            primary_hints,
            origin_kind,
            origin_pointer,
            origin_len,
            origin_capacity,
            vector_pointer,
            vector_capacity,
            nodes,
        }
    }

    fn nodes_by_id(snapshot: &DocumentSnapshot) -> Vec<NodeSnapshot> {
        let mut nodes = snapshot.nodes.clone();
        nodes.sort_by_key(|node| node.id);
        nodes
    }

    fn assert_only_order_primary_and_dirty_changed(
        before: &DocumentSnapshot,
        after: &DocumentSnapshot,
    ) {
        assert_eq!(after.logical_len, before.logical_len);
        assert_eq!(after.file, before.file);
        assert_eq!(after.compatibility, before.compatibility);
        assert_eq!(after.trailing, before.trailing);
        assert_eq!(after.next_id, before.next_id);
        assert_eq!(after.origin_kind, before.origin_kind);
        assert_eq!(after.origin_pointer, before.origin_pointer);
        assert_eq!(after.origin_len, before.origin_len);
        assert_eq!(after.origin_capacity, before.origin_capacity);
        assert_eq!(after.vector_pointer, before.vector_pointer);
        assert_eq!(after.vector_capacity, before.vector_capacity);
        assert_eq!(nodes_by_id(after), nodes_by_id(before));
    }

    #[test]
    fn set_primary_rotates_a_duplicate_without_reallocating_and_clear_is_exact() {
        let source = encode_chunks(&[
            (TYPE_A.raw(), 0, b"a0"),
            (TYPE_B.raw(), 0, b"b"),
            (TYPE_A.raw(), 0, b"a1"),
            (TYPE_C.raw(), 0, b"c"),
            (TYPE_A.raw(), 0, b"a2"),
        ]);
        let mut document = Document::open(&source).unwrap();
        let before = snapshot(&document);

        document
            .set_primary_with_hints(id(4), EXPLICIT_HINTS)
            .unwrap();
        let selected = snapshot(&document);
        assert_eq!(document.primary(), Some(id(4)));
        assert_eq!(ids(&document), [id(4), id(0), id(1), id(2), id(3)]);
        assert!(document.is_dirty());
        assert_only_order_primary_and_dirty_changed(&before, &selected);

        document.dirty = false;
        let before_noop = snapshot(&document);
        document
            .set_primary_with_hints(id(4), EXPLICIT_HINTS)
            .unwrap();
        assert_eq!(snapshot(&document), before_noop);

        document.clear_primary().unwrap();
        assert_eq!(document.primary(), None);
        assert_eq!(ids(&document), [id(4), id(0), id(1), id(2), id(3)]);
        assert!(document.is_dirty());
        let cleared = snapshot(&document);
        assert_only_order_primary_and_dirty_changed(&before_noop, &cleared);

        document.dirty = false;
        let before_clear_noop = snapshot(&document);
        document.clear_primary().unwrap();
        assert_eq!(snapshot(&document), before_clear_noop);

        let first_owned = alloc::vec![1, 2, 3];
        let first_pointer = first_owned.as_ptr();
        let first_capacity = first_owned.capacity();
        let selected_owned = alloc::vec![7, 8, 9, 10];
        let selected_pointer = selected_owned.as_ptr();
        let selected_capacity = selected_owned.capacity();
        let mut mixed = Document::new_chunk();
        let first = mixed
            .push_raw(RawChunkInput {
                chunk_type: TYPE_A,
                flags: ChunkFlags::NONE,
                payload: PayloadInput::Owned(first_owned),
                policy: explicit_policy(),
            })
            .unwrap();
        mixed
            .push_raw(raw(TYPE_B, b"borrowed", explicit_policy()))
            .unwrap();
        let selected = mixed
            .push_raw(RawChunkInput {
                chunk_type: TYPE_A,
                flags: ChunkFlags::NONE,
                payload: PayloadInput::Owned(selected_owned),
                policy: explicit_policy(),
            })
            .unwrap();
        mixed.dirty = false;
        let before_mixed = snapshot(&mixed);
        mixed
            .set_primary_with_hints(selected, EXPLICIT_HINTS)
            .unwrap();
        let after_mixed = snapshot(&mixed);
        assert_eq!(ids(&mixed), [selected, first, id(1)]);
        assert_only_order_primary_and_dirty_changed(&before_mixed, &after_mixed);
        let by_id = nodes_by_id(&after_mixed);
        assert_eq!(by_id[0].payload_pointer, first_pointer as usize);
        assert_eq!(by_id[0].owned_capacity, Some(first_capacity));
        assert_eq!(by_id[2].payload_pointer, selected_pointer as usize);
        assert_eq!(by_id[2].owned_capacity, Some(selected_capacity));
    }

    #[test]
    fn set_primary_from_none_rotates_a_later_duplicate_of_another_type() {
        let mut source = encode_chunks(&[
            (TYPE_A.raw(), 0, b"a0"),
            (TYPE_B.raw(), 0, b"b0"),
            (TYPE_C.raw(), 0, b"c"),
            (TYPE_A.raw(), 0, b"a1"),
            (TYPE_B.raw(), 0, b"b1"),
        ]);
        clear_wire_primary(&mut source);
        let mut document = Document::open(&source).unwrap();
        let before = snapshot(&document);
        assert_eq!(document.primary(), None);

        document
            .set_primary_with_hints(id(4), EXPLICIT_HINTS)
            .unwrap();
        let after = snapshot(&document);
        assert_eq!(document.primary(), Some(id(4)));
        assert_eq!(ids(&document), [id(0), id(4), id(1), id(2), id(3)]);
        assert_eq!(types(&document), [TYPE_A, TYPE_B, TYPE_B, TYPE_C, TYPE_A]);
        assert_only_order_primary_and_dirty_changed(&before, &after);
    }

    #[test]
    fn primary_selection_validates_layout_and_id_before_changing_state() {
        let flat_source = encode_flat(&FlatImageInput {
            width: 1,
            height: 1,
            stride: 1,
            format: crate::ColorFormat::A8,
            main: &[7],
            extra: None,
        });
        let mut flat = Document::open(&flat_source).unwrap();
        let before_flat = snapshot(&flat);
        assert_eq!(flat.set_primary(id(0)), Err(EditError::ChunkLayoutRequired));
        assert_eq!(snapshot(&flat), before_flat);
        assert_eq!(flat.clear_primary(), Err(EditError::ChunkLayoutRequired));
        assert_eq!(snapshot(&flat), before_flat);

        let mut source = encode_chunks(&[(TYPE_A.raw(), 0, b"a")]);
        clear_wire_primary(&mut source);
        let mut document = Document::open(&source).unwrap();
        let before = snapshot(&document);
        document.clear_primary().unwrap();
        assert_eq!(snapshot(&document), before);
        assert_eq!(document.set_primary(id(99)), Err(EditError::InvalidChunkId));
        assert_eq!(snapshot(&document), before);

        let mut empty = Document::new_chunk();
        empty.dirty = false;
        let before_empty = snapshot(&empty);
        empty.clear_primary().unwrap();
        assert_eq!(snapshot(&empty), before_empty);
        assert_eq!(empty.set_primary(id(0)), Err(EditError::InvalidChunkId));
        assert_eq!(snapshot(&empty), before_empty);
    }

    #[test]
    fn documents_without_a_primary_do_not_apply_shadow_restrictions() {
        let mut source = encode_chunks(&[
            (TYPE_A.raw(), 0, b"a0"),
            (TYPE_B.raw(), 0, b"b"),
            (TYPE_A.raw(), 0, b"a1"),
        ]);
        clear_wire_primary(&mut source);
        let mut document = Document::open(&source).unwrap();
        assert_eq!(document.primary(), None);

        let inserted = document
            .insert_before(id(0), raw(TYPE_A, b"new", explicit_policy()))
            .unwrap();
        assert_eq!(ids(&document), [inserted, id(0), id(1), id(2)]);
        assert_eq!(document.primary(), None);

        document.dirty = false;
        document.move_after(inserted, id(2)).unwrap();
        assert_eq!(ids(&document), [id(0), id(1), id(2), inserted]);
        assert!(document.is_dirty());
        assert_eq!(document.primary(), None);

        document.dirty = false;
        document.set_type(id(1), TYPE_A, explicit_policy()).unwrap();
        assert_eq!(types(&document), [TYPE_A, TYPE_A, TYPE_A, TYPE_A]);
        assert!(document.is_dirty());
        assert_eq!(document.primary(), None);
    }

    #[test]
    fn insert_rejects_both_shadowing_positions_before_policy_and_reserve() {
        let mut source = encode_chunks(&[
            (TYPE_B.raw(), 0, b"b"),
            (TYPE_A.raw(), 0, b"primary"),
            (TYPE_C.raw(), 0, b"c"),
            (TYPE_A.raw(), 0, b"duplicate"),
        ]);
        set_wire_primary(&mut source, TYPE_A);
        let mut document = Document::open(&source).unwrap();
        let before = snapshot(&document);

        assert_eq!(
            document.insert_before(id(1), raw(TYPE_A, b"new", RawChunkPolicy::infer())),
            Err(EditError::WouldShadowPrimary)
        );
        assert_eq!(snapshot(&document), before);
        assert_eq!(
            document.insert_after(id(0), raw(TYPE_A, b"new", RawChunkPolicy::infer())),
            Err(EditError::WouldShadowPrimary)
        );
        assert_eq!(snapshot(&document), before);

        document.next_id = u32::MAX;
        let exhausted = snapshot(&document);
        assert_eq!(
            document.insert_before(id(1), raw(TYPE_A, b"new", RawChunkPolicy::infer())),
            Err(EditError::ChunkIdExhausted)
        );
        assert_eq!(snapshot(&document), exhausted);
        document.next_id = before.next_id;

        let inserted = document
            .insert_after(id(1), raw(TYPE_A, b"allowed", explicit_policy()))
            .unwrap();
        assert_eq!(inserted, id(4));
        assert_eq!(document.primary(), Some(id(1)));
        assert_eq!(ids(&document), [id(0), id(1), id(4), id(2), id(3)]);
        assert_eq!(types(&document), [TYPE_B, TYPE_A, TYPE_A, TYPE_C, TYPE_A]);
    }

    #[test]
    fn moves_project_both_directions_and_keep_self_and_adjacent_noops_clean() {
        let mut source = encode_chunks(&[
            (TYPE_B.raw(), 0, b"b"),
            (TYPE_A.raw(), 0, b"primary"),
            (TYPE_C.raw(), 0, b"c"),
            (TYPE_A.raw(), 0, b"duplicate"),
            (TYPE_D.raw(), 0, b"d"),
        ]);
        set_wire_primary(&mut source, TYPE_A);
        let mut document = Document::open(&source).unwrap();
        let before = snapshot(&document);

        assert_eq!(
            document.move_before(id(3), id(1)),
            Err(EditError::WouldShadowPrimary)
        );
        assert_eq!(snapshot(&document), before);
        assert_eq!(
            document.move_after(id(3), id(0)),
            Err(EditError::WouldShadowPrimary)
        );
        assert_eq!(snapshot(&document), before);
        assert_eq!(
            document.move_after(id(1), id(3)),
            Err(EditError::WouldShadowPrimary)
        );
        assert_eq!(snapshot(&document), before);
        assert_eq!(
            document.move_before(id(1), id(4)),
            Err(EditError::WouldShadowPrimary)
        );
        assert_eq!(snapshot(&document), before);
        assert_eq!(
            document.move_before(id(99), id(1)),
            Err(EditError::InvalidChunkId)
        );
        assert_eq!(snapshot(&document), before);
        assert_eq!(
            document.move_after(id(3), id(99)),
            Err(EditError::InvalidChunkId)
        );
        assert_eq!(snapshot(&document), before);

        document.move_after(id(1), id(2)).unwrap();
        let moved = snapshot(&document);
        assert_eq!(ids(&document), [id(0), id(2), id(1), id(3), id(4)]);
        assert_eq!(document.primary(), Some(id(1)));
        assert!(document.is_dirty());
        assert_only_order_primary_and_dirty_changed(&before, &moved);

        document.dirty = false;
        let before_noops = snapshot(&document);
        document.move_before(id(1), id(3)).unwrap();
        assert_eq!(snapshot(&document), before_noops);
        document.move_after(id(3), id(1)).unwrap();
        assert_eq!(snapshot(&document), before_noops);
        document.move_before(id(1), id(1)).unwrap();
        assert_eq!(snapshot(&document), before_noops);
        document.move_after(id(1), id(1)).unwrap();
        assert_eq!(snapshot(&document), before_noops);
    }

    #[test]
    fn type_changes_project_primary_identity_before_payload_policy() {
        let mut source = encode_chunks(&[
            (TYPE_B.raw(), 0, b"b"),
            (TYPE_A.raw(), 0, b"primary"),
            (TYPE_C.raw(), 0, b"c"),
            (TYPE_D.raw(), 0, b"d"),
        ]);
        set_wire_primary(&mut source, TYPE_A);
        let mut document = Document::open(&source).unwrap();
        let before = snapshot(&document);

        assert_eq!(
            document.set_type(id(0), TYPE_A, RawChunkPolicy::infer()),
            Err(EditError::WouldShadowPrimary)
        );
        assert_eq!(snapshot(&document), before);
        assert_eq!(
            document.set_type(id(1), TYPE_B, RawChunkPolicy::infer()),
            Err(EditError::WouldShadowPrimary)
        );
        assert_eq!(snapshot(&document), before);

        document.set_type(id(2), TYPE_A, explicit_policy()).unwrap();
        assert_eq!(types(&document), [TYPE_B, TYPE_A, TYPE_A, TYPE_D]);
        assert_eq!(document.primary(), Some(id(1)));
        assert!(document.is_dirty());
        let changed_duplicate = snapshot(&document);
        assert_eq!(changed_duplicate.vector_pointer, before.vector_pointer);
        assert_eq!(changed_duplicate.vector_capacity, before.vector_capacity);
        assert_eq!(changed_duplicate.next_id, before.next_id);
        assert_eq!(
            changed_duplicate.nodes[2].payload_pointer,
            before.nodes[2].payload_pointer
        );

        document.dirty = false;
        let before_exact = snapshot(&document);
        document
            .set_type(id(1), TYPE_A, RawChunkPolicy::infer())
            .unwrap();
        assert_eq!(snapshot(&document), before_exact);

        document.set_type(id(1), TYPE_D, explicit_policy()).unwrap();
        assert_eq!(types(&document), [TYPE_B, TYPE_D, TYPE_A, TYPE_D]);
        assert_eq!(document.primary(), Some(id(1)));
    }
}
