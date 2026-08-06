use super::payload::{ResolvedNodePayload, resolve_node_payload};
use super::primary::{PrimaryProjection, changed_primary_hint_state, ensure_primary_projection};
use super::raw::{CriticalAssumption, RawChunkPolicy, RelocationAssumption, ReservedBitsPolicy};
use super::{ChunkNode, Document, DocumentState, RewriteCapability};
use crate::{ChunkFlags, ChunkId, ChunkType, EditError};

#[cfg(test)]
use super::PayloadStorage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EvaluatedDescriptor {
    pub(super) chunk_type: ChunkType,
    pub(super) flags: ChunkFlags,
    pub(super) capability: RewriteCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EvaluatedFlags {
    pub(super) flags: ChunkFlags,
    preserve_reserved_bits: bool,
}

pub(super) fn evaluate_flags(
    flags: ChunkFlags,
    policy: ReservedBitsPolicy,
) -> Result<EvaluatedFlags, EditError> {
    let reserved_bits = flags.bits() & !ChunkFlags::CRITICAL.bits();
    let (flags, preserve_reserved_bits) = match (reserved_bits, policy) {
        (0, _) => (flags, false),
        (_, ReservedBitsPolicy::Preserve) => (flags, true),
        (_, ReservedBitsPolicy::Normalize) => (
            ChunkFlags::from_bits_retain(flags.bits() & ChunkFlags::CRITICAL.bits()),
            false,
        ),
        (_, ReservedBitsPolicy::Reject) => {
            return Err(EditError::ReservedFlagBits {
                bits: reserved_bits,
            });
        }
    };
    Ok(EvaluatedFlags {
        flags,
        preserve_reserved_bits,
    })
}

/// Evaluates a complete descriptor candidate without changing document state.
pub(super) fn evaluate_descriptor(
    chunk_type: ChunkType,
    flags: ChunkFlags,
    payload: &[u8],
    policy: RawChunkPolicy,
) -> Result<EvaluatedDescriptor, EditError> {
    evaluate_descriptor_at(chunk_type, flags, payload, 0, policy)
}

pub(super) fn evaluate_descriptor_at(
    chunk_type: ChunkType,
    flags: ChunkFlags,
    payload: &[u8],
    payload_offset: u32,
    policy: RawChunkPolicy,
) -> Result<EvaluatedDescriptor, EditError> {
    let flags = evaluate_flags(flags, policy.reserved_flag_bits)?;
    evaluate_resolved_descriptor_with_flags(
        chunk_type,
        flags,
        ResolvedNodePayload::Contiguous {
            bytes: payload,
            absolute_offset: payload_offset,
        },
        policy,
    )
}

pub(super) fn evaluate_descriptor_with_flags(
    chunk_type: ChunkType,
    evaluated_flags: EvaluatedFlags,
    payload: &[u8],
    policy: RawChunkPolicy,
) -> Result<EvaluatedDescriptor, EditError> {
    evaluate_resolved_descriptor_with_flags(
        chunk_type,
        evaluated_flags,
        ResolvedNodePayload::Contiguous {
            bytes: payload,
            absolute_offset: 0,
        },
        policy,
    )
}

fn evaluate_resolved_descriptor_with_flags(
    chunk_type: ChunkType,
    evaluated_flags: EvaluatedFlags,
    payload: ResolvedNodePayload<'_>,
    policy: RawChunkPolicy,
) -> Result<EvaluatedDescriptor, EditError> {
    let flags = evaluated_flags.flags;
    let known_contract = if chunk_type == ChunkType::IMAGE {
        match payload.validate_image_contract() {
            Ok(_) => true,
            Err(_) if matches!(policy.relocation, RelocationAssumption::AssumeRelocatable) => false,
            Err(error) => return Err(EditError::InvalidPayload(error)),
        }
    } else {
        false
    };

    let relocatable =
        known_contract || matches!(policy.relocation, RelocationAssumption::AssumeRelocatable);
    if !relocatable {
        return Err(EditError::RelocationAssumptionRequired { chunk_type });
    }

    let critical_understood = known_contract
        || matches!(
            policy.critical_semantics,
            CriticalAssumption::AssumeCriticalUnderstood
        );
    if flags.is_critical() && !critical_understood {
        return Err(EditError::CriticalAssumptionRequired { chunk_type });
    }

    Ok(EvaluatedDescriptor {
        chunk_type,
        flags,
        capability: RewriteCapability::new(
            relocatable,
            critical_understood,
            evaluated_flags.preserve_reserved_bits,
        ),
    })
}

pub(super) fn grant_open_descriptor(
    chunk_type: ChunkType,
    flags: ChunkFlags,
    known_contract: bool,
    policy: Option<RawChunkPolicy>,
    allow_normalize: bool,
) -> (EvaluatedDescriptor, bool) {
    let reserved_bits = flags.bits() & !ChunkFlags::CRITICAL.bits();
    let evaluated_flags = match policy.map(|policy| policy.reserved_flag_bits) {
        Some(ReservedBitsPolicy::Preserve) if reserved_bits != 0 => {
            evaluate_flags(flags, ReservedBitsPolicy::Preserve)
                .expect("preserving reserved bits cannot fail")
        }
        Some(ReservedBitsPolicy::Normalize) if reserved_bits != 0 && allow_normalize => {
            evaluate_flags(flags, ReservedBitsPolicy::Normalize)
                .expect("normalizing reserved bits cannot fail")
        }
        _ => EvaluatedFlags {
            flags,
            preserve_reserved_bits: false,
        },
    };
    let policy = policy.unwrap_or_else(RawChunkPolicy::infer);
    let relocatable =
        known_contract || matches!(policy.relocation, RelocationAssumption::AssumeRelocatable);
    let critical_understood = known_contract
        || matches!(
            policy.critical_semantics,
            CriticalAssumption::AssumeCriticalUnderstood
        );
    let normalized = evaluated_flags.flags != flags;
    (
        EvaluatedDescriptor {
            chunk_type,
            flags: evaluated_flags.flags,
            capability: RewriteCapability::new(
                relocatable,
                critical_understood,
                evaluated_flags.preserve_reserved_bits,
            ),
        },
        normalized,
    )
}

impl Document<'_> {
    /// Changes the type of `id` after validating its existing encoded payload.
    pub fn set_type(
        &mut self,
        id: ChunkId,
        chunk_type: ChunkType,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let index = descriptor_chunk_index(&self.state, id)?;
        let existing_type = chunk_node(&self.state, index).chunk_type;
        if existing_type == chunk_type {
            return Ok(());
        }

        let DocumentState::Chunk(chunks) = &self.state else {
            unreachable!("layout checked before projecting descriptor edit");
        };
        ensure_primary_projection(chunks, PrimaryProjection::SetType { index, chunk_type })?;

        let candidate = {
            let node = chunk_node(&self.state, index);
            let payload = descriptor_payload(self, node)?;
            let flags = evaluate_flags(node.flags, policy.reserved_flag_bits)?;
            evaluate_resolved_descriptor_with_flags(chunk_type, flags, payload, policy)?
        };
        self.apply_type_descriptor(index, candidate);
        Ok(())
    }

    /// Changes the flags of `id` after validating its existing encoded payload.
    pub fn set_flags(
        &mut self,
        id: ChunkId,
        flags: ChunkFlags,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let index = descriptor_chunk_index(&self.state, id)?;
        let existing_flags = chunk_node(&self.state, index).flags;
        if existing_flags == flags
            && !matches!(policy.reserved_flag_bits, ReservedBitsPolicy::Normalize)
        {
            return Ok(());
        }

        let evaluated_flags = evaluate_flags(flags, policy.reserved_flag_bits)?;
        if evaluated_flags.flags == existing_flags {
            return Ok(());
        }
        let candidate = {
            let node = chunk_node(&self.state, index);
            let payload = descriptor_payload(self, node)?;
            evaluate_resolved_descriptor_with_flags(
                node.chunk_type,
                evaluated_flags,
                payload,
                policy,
            )?
        };
        self.apply_flags_descriptor(index, candidate);
        Ok(())
    }

    /// Re-evaluates the rewrite capability of one existing raw node.
    ///
    /// Capability-only changes do not alter encoded output and therefore do
    /// not mark the document dirty. Normalizing reserved flag bits does.
    pub fn set_raw_policy(&mut self, id: ChunkId, policy: RawChunkPolicy) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let index = descriptor_chunk_index(&self.state, id)?;
        let candidate = {
            let node = chunk_node(&self.state, index);
            let payload = descriptor_payload(self, node)?;
            let flags = evaluate_flags(node.flags, policy.reserved_flag_bits)?;
            evaluate_resolved_descriptor_with_flags(node.chunk_type, flags, payload, policy)?
        };

        let (exact, flags_changed) = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout was checked before planning raw policy");
            };
            let node = &chunks.chunks[index];
            let exact = node.flags == candidate.flags && node.capability == candidate.capability;
            let flags_changed = node.flags != candidate.flags;
            (exact, flags_changed)
        };
        if exact {
            return Ok(());
        }

        let DocumentState::Chunk(chunks) = &mut self.state else {
            unreachable!("layout was checked before applying raw policy");
        };
        let node = &mut chunks.chunks[index];
        node.flags = candidate.flags;
        node.capability = candidate.capability;
        if flags_changed {
            self.dirty = true;
        }
        Ok(())
    }

    fn apply_type_descriptor(&mut self, index: usize, candidate: EvaluatedDescriptor) {
        let primary_hints = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout was checked before planning descriptor edit");
            };
            let node = &chunks.chunks[index];
            if chunks.primary == Some(node.id) {
                let payload = descriptor_payload(self, node)
                    .expect("live document payload must remain resolvable");
                Some(changed_primary_hint_state(candidate.chunk_type, payload))
            } else {
                None
            }
        };
        let DocumentState::Chunk(chunks) = &mut self.state else {
            unreachable!("layout was checked before applying descriptor edit");
        };
        let node = &mut chunks.chunks[index];
        node.chunk_type = candidate.chunk_type;
        node.flags = candidate.flags;
        node.capability = candidate.capability;
        if let Some(primary_hints) = primary_hints {
            chunks.primary_hints = primary_hints;
        }
        self.dirty = true;
    }

    fn apply_flags_descriptor(&mut self, index: usize, candidate: EvaluatedDescriptor) {
        let DocumentState::Chunk(chunks) = &mut self.state else {
            unreachable!("layout was checked before applying descriptor edit");
        };
        let node = &mut chunks.chunks[index];
        node.chunk_type = candidate.chunk_type;
        node.flags = candidate.flags;
        node.capability = candidate.capability;
        self.dirty = true;
    }
}

fn descriptor_chunk_index(state: &DocumentState<'_>, id: ChunkId) -> Result<usize, EditError> {
    let DocumentState::Chunk(chunks) = state else {
        return Err(EditError::ChunkLayoutRequired);
    };
    chunks
        .chunks
        .iter()
        .position(|node| node.id == id)
        .ok_or(EditError::InvalidChunkId)
}

fn chunk_node<'document, 'source>(
    state: &'document DocumentState<'source>,
    index: usize,
) -> &'document ChunkNode<'source> {
    let DocumentState::Chunk(chunks) = state else {
        unreachable!("layout was checked before resolving chunk node");
    };
    &chunks.chunks[index]
}

pub(super) fn descriptor_payload<'document>(
    document: &'document Document<'_>,
    node: &'document ChunkNode<'_>,
) -> Result<ResolvedNodePayload<'document>, EditError> {
    resolve_node_payload(document, node).map_err(EditError::InvalidPayload)
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::document::source::SourceRange;
    use crate::{
        CHUNK_FILE_HEADER_LEN, ChunkFlags, ColorFormat, FlatImageInput, ImageChunkInput,
        PayloadInput, RawChunkInput, encode_chunk_image, encode_chunks, encode_flat,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct StorageSnapshot {
        kind: u8,
        source_range: Option<SourceRange>,
        pointer: usize,
        len: usize,
        owned_capacity: Option<usize>,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct NodeSnapshot {
        id: ChunkId,
        chunk_type: ChunkType,
        flags: ChunkFlags,
        capability: RewriteCapability,
        storage: StorageSnapshot,
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

    const fn policy(
        critical_semantics: CriticalAssumption,
        reserved_flag_bits: ReservedBitsPolicy,
    ) -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics,
            reserved_flag_bits,
        }
    }

    const fn explicit_policy(reserved_flag_bits: ReservedBitsPolicy) -> RawChunkPolicy {
        policy(
            CriticalAssumption::AssumeCriticalUnderstood,
            reserved_flag_bits,
        )
    }

    fn raw<'a>(
        chunk_type: ChunkType,
        flags: ChunkFlags,
        payload: PayloadInput<'a>,
        policy: RawChunkPolicy,
    ) -> RawChunkInput<'a> {
        RawChunkInput {
            chunk_type,
            flags,
            payload,
            policy,
        }
    }

    fn valid_image_payload() -> Vec<u8> {
        let file = encode_chunk_image(&ImageChunkInput {
            width: 2,
            height: 2,
            format: ColorFormat::A8,
            stride: 2,
            main: &[1, 2, 3, 4],
            extra: None,
        });
        let entry = CHUNK_FILE_HEADER_LEN;
        let start = u32::from_le_bytes(file[entry + 4..entry + 8].try_into().unwrap()) as usize;
        let len = u32::from_le_bytes(file[entry + 8..entry + 12].try_into().unwrap()) as usize;
        file[start..start + len].to_vec()
    }

    fn set_primary(source: &mut [u8], chunk_type: ChunkType) {
        source[20..22].copy_from_slice(&chunk_type.raw().to_le_bytes());
        let checksum = crate::crc32(&source[..40]);
        source[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    fn nodes<'document, 'source>(
        document: &'document Document<'source>,
    ) -> &'document [ChunkNode<'source>] {
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("expected CHUNK document");
        };
        chunks.chunks.as_slice()
    }

    fn node<'document, 'source>(
        document: &'document Document<'source>,
        id: ChunkId,
    ) -> &'document ChunkNode<'source> {
        nodes(document).iter().find(|node| node.id == id).unwrap()
    }

    fn storage_snapshot(document: &Document<'_>, node: &ChunkNode<'_>) -> StorageSnapshot {
        let (kind, source_range, bytes, owned_capacity) = match &node.payload {
            PayloadStorage::SourceRange(range) => (
                0,
                Some(*range),
                document.origin.resolve(*range).unwrap(),
                None,
            ),
            PayloadStorage::Borrowed(bytes) => (1, None, *bytes, None),
            PayloadStorage::Owned(bytes) => (2, None, bytes.as_slice(), Some(bytes.capacity())),
            PayloadStorage::PromotedFlat => (3, None, &[] as &[u8], None),
        };
        StorageSnapshot {
            kind,
            source_range,
            pointer: bytes.as_ptr() as usize,
            len: bytes.len(),
            owned_capacity,
        }
    }

    fn node_snapshot(document: &Document<'_>, node: &ChunkNode<'_>) -> NodeSnapshot {
        NodeSnapshot {
            id: node.id,
            chunk_type: node.chunk_type,
            flags: node.flags,
            capability: node.capability,
            storage: storage_snapshot(document, node),
        }
    }

    fn snapshot(document: &Document<'_>) -> DocumentSnapshot {
        let nodes = nodes(document);
        let vector_capacity = match &document.state {
            DocumentState::Chunk(chunks) => chunks.chunks.capacity(),
            _ => unreachable!(),
        };
        DocumentSnapshot {
            nodes: nodes
                .iter()
                .map(|node| node_snapshot(document, node))
                .collect(),
            vector_pointer: nodes.as_ptr() as usize,
            vector_capacity,
            primary: document.primary(),
            next_id: document.next_id,
            dirty: document.dirty,
        }
    }

    #[test]
    fn set_type_accepts_open_type_boundaries_and_keeps_primary_identity() {
        let mut source = encode_chunks(&[(ChunkType::META.raw(), 0, b"opaque")]);
        set_primary(&mut source, ChunkType::META);
        let mut document = Document::open(&source).unwrap();
        let id = document.primary().unwrap();
        let next_id = document.next_id;

        for chunk_type in [
            ChunkType::new(0xbeef).unwrap(),
            ChunkType::new(0xffff).unwrap(),
            ChunkType::IMAGE,
        ] {
            document.dirty = false;
            document
                .set_type(id, chunk_type, explicit_policy(ReservedBitsPolicy::Reject))
                .unwrap();
            assert_eq!(document.get(id).unwrap().chunk_type(), chunk_type);
            assert_eq!(document.primary(), Some(id));
            assert_eq!(document.next_id, next_id);
            assert!(document.is_dirty());
        }
        assert_eq!(ChunkType::IMAGE.raw(), 1);
        assert_eq!(nodes(&document).len(), 1);
    }

    #[test]
    fn valid_image_contract_is_inferred_for_type_and_critical_flag_edits() {
        let payload = valid_image_payload();
        let payload_pointer = payload.as_ptr();
        let payload_capacity = payload.capacity();
        let mut document = Document::new_chunk();
        let id = document
            .push_raw(raw(
                ChunkType::META,
                ChunkFlags::NONE,
                PayloadInput::Owned(payload),
                explicit_policy(ReservedBitsPolicy::Reject),
            ))
            .unwrap();
        document.dirty = false;

        document
            .set_type(id, ChunkType::IMAGE, RawChunkPolicy::infer())
            .unwrap();
        let image = node(&document, id);
        assert_eq!(image.chunk_type, ChunkType::IMAGE);
        assert!(image.capability.is_relocatable());
        assert!(image.capability.critical_understood());
        assert!(!image.capability.preserves_reserved_bits());
        assert_eq!(
            storage_snapshot(&document, image).pointer,
            payload_pointer as usize
        );
        assert_eq!(
            storage_snapshot(&document, image).owned_capacity,
            Some(payload_capacity)
        );

        document.dirty = false;
        document
            .set_flags(id, ChunkFlags::CRITICAL, RawChunkPolicy::infer())
            .unwrap();
        let image = node(&document, id);
        assert_eq!(image.flags, ChunkFlags::CRITICAL);
        assert!(image.capability.is_relocatable());
        assert!(image.capability.critical_understood());
        assert!(document.is_dirty());
    }

    #[test]
    fn malformed_image_requires_relocation_and_then_critical_understanding() {
        let mut document = Document::new_chunk();
        let id = document
            .push_raw(raw(
                ChunkType::META,
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"not an image"),
                explicit_policy(ReservedBitsPolicy::Reject),
            ))
            .unwrap();
        document.dirty = false;
        let before = snapshot(&document);

        for chunk_type in [ChunkType::FONT, ChunkType::new(0xbeef).unwrap()] {
            assert_eq!(
                document.set_type(id, chunk_type, RawChunkPolicy::infer()),
                Err(EditError::RelocationAssumptionRequired { chunk_type })
            );
            assert_eq!(snapshot(&document), before);
        }

        assert!(matches!(
            document.set_type(id, ChunkType::IMAGE, RawChunkPolicy::infer()),
            Err(EditError::InvalidPayload(_))
        ));
        assert_eq!(snapshot(&document), before);

        document
            .set_type(
                id,
                ChunkType::IMAGE,
                policy(CriticalAssumption::Infer, ReservedBitsPolicy::Reject),
            )
            .unwrap();
        assert_eq!(node(&document, id).chunk_type, ChunkType::IMAGE);
        assert!(!node(&document, id).capability.critical_understood());

        document.dirty = false;
        let before_critical = snapshot(&document);
        assert_eq!(
            document.set_flags(
                id,
                ChunkFlags::CRITICAL,
                policy(CriticalAssumption::Infer, ReservedBitsPolicy::Reject),
            ),
            Err(EditError::CriticalAssumptionRequired {
                chunk_type: ChunkType::IMAGE,
            })
        );
        assert_eq!(snapshot(&document), before_critical);

        document
            .set_flags(
                id,
                ChunkFlags::CRITICAL,
                explicit_policy(ReservedBitsPolicy::Reject),
            )
            .unwrap();
        assert_eq!(node(&document, id).flags, ChunkFlags::CRITICAL);
        assert!(node(&document, id).capability.critical_understood());
        assert!(document.is_dirty());
    }

    #[test]
    fn reserved_flag_policies_normalize_descriptors_and_replace_capability() {
        let reserved_critical = ChunkFlags::from_bits_retain(0xa501);
        let reserved_only = ChunkFlags::from_bits_retain(0xa500);
        let mut document = Document::new_chunk();
        let id = document
            .push_raw(raw(
                ChunkType::new(0xbeef).unwrap(),
                reserved_critical,
                PayloadInput::Borrowed(b"opaque"),
                explicit_policy(ReservedBitsPolicy::Preserve),
            ))
            .unwrap();
        let initial = node(&document, id).capability;
        assert!(initial.is_relocatable());
        assert!(initial.critical_understood());
        assert!(initial.preserves_reserved_bits());

        document.dirty = false;
        document
            .set_type(
                id,
                ChunkType::new(0xffff).unwrap(),
                explicit_policy(ReservedBitsPolicy::Normalize),
            )
            .unwrap();
        let normalized_type = node(&document, id);
        assert_eq!(normalized_type.chunk_type, ChunkType::new(0xffff).unwrap());
        assert_eq!(normalized_type.flags, ChunkFlags::CRITICAL);
        assert!(!normalized_type.capability.preserves_reserved_bits());
        assert!(document.is_dirty());

        document.dirty = false;
        document
            .set_flags(
                id,
                ChunkFlags::NONE,
                policy(CriticalAssumption::Infer, ReservedBitsPolicy::Reject),
            )
            .unwrap();
        let cleared = node(&document, id);
        assert!(cleared.capability.is_relocatable());
        assert!(!cleared.capability.critical_understood());
        assert!(!cleared.capability.preserves_reserved_bits());

        document.dirty = false;
        let before_reject = snapshot(&document);
        assert_eq!(
            document.set_flags(
                id,
                reserved_only,
                explicit_policy(ReservedBitsPolicy::Reject),
            ),
            Err(EditError::ReservedFlagBits { bits: 0xa500 })
        );
        assert_eq!(snapshot(&document), before_reject);

        document
            .set_flags(
                id,
                reserved_only,
                policy(CriticalAssumption::Infer, ReservedBitsPolicy::Preserve),
            )
            .unwrap();
        assert_eq!(node(&document, id).flags, reserved_only);
        assert!(node(&document, id).capability.preserves_reserved_bits());

        document.dirty = false;
        document
            .set_flags(
                id,
                reserved_critical,
                explicit_policy(ReservedBitsPolicy::Normalize),
            )
            .unwrap();
        assert_eq!(node(&document, id).flags, ChunkFlags::CRITICAL);
        assert!(!node(&document, id).capability.preserves_reserved_bits());
        assert!(document.is_dirty());
    }

    #[test]
    fn exact_descriptor_noops_skip_policy_and_normalize_resolves_effective_flags() {
        let flags = ChunkFlags::from_bits_retain(0xa501);
        let chunk_type = ChunkType::new(0xbeef).unwrap();
        let mut document = Document::new_chunk();
        let id = document
            .push_raw(raw(
                chunk_type,
                flags,
                PayloadInput::Borrowed(b"opaque"),
                explicit_policy(ReservedBitsPolicy::Preserve),
            ))
            .unwrap();
        document.dirty = false;
        let original = snapshot(&document);

        assert_eq!(
            document.set_type(id, chunk_type, RawChunkPolicy::infer()),
            Ok(())
        );
        assert_eq!(
            document.set_flags(id, flags, RawChunkPolicy::infer()),
            Ok(())
        );
        assert_eq!(snapshot(&document), original);

        assert_eq!(
            document.replace_raw(
                id,
                PayloadInput::Borrowed(b"replacement"),
                RawChunkPolicy {
                    relocation: RelocationAssumption::Infer,
                    critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
                    reserved_flag_bits: ReservedBitsPolicy::Normalize,
                },
            ),
            Err(EditError::ReservedFlagBits { bits: 0xa500 })
        );
        assert_eq!(snapshot(&document), original);

        assert_eq!(
            document.set_flags(
                id,
                flags,
                RawChunkPolicy {
                    relocation: RelocationAssumption::Infer,
                    critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
                    reserved_flag_bits: ReservedBitsPolicy::Normalize,
                },
            ),
            Err(EditError::RelocationAssumptionRequired { chunk_type })
        );
        assert_eq!(snapshot(&document), original);

        let mut normalized_noop = Document::new_chunk();
        let normalized_id = normalized_noop
            .push_raw(raw(
                chunk_type,
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"opaque"),
                explicit_policy(ReservedBitsPolicy::Reject),
            ))
            .unwrap();
        normalized_noop.dirty = false;
        let before_normalized_noop = snapshot(&normalized_noop);
        normalized_noop
            .set_flags(
                normalized_id,
                ChunkFlags::NONE,
                RawChunkPolicy {
                    relocation: RelocationAssumption::Infer,
                    critical_semantics: CriticalAssumption::Infer,
                    reserved_flag_bits: ReservedBitsPolicy::Normalize,
                },
            )
            .unwrap();
        assert_eq!(snapshot(&normalized_noop), before_normalized_noop);
        normalized_noop
            .set_flags(
                normalized_id,
                ChunkFlags::from_bits_retain(0xa500),
                RawChunkPolicy {
                    relocation: RelocationAssumption::Infer,
                    critical_semantics: CriticalAssumption::Infer,
                    reserved_flag_bits: ReservedBitsPolicy::Normalize,
                },
            )
            .unwrap();
        assert_eq!(snapshot(&normalized_noop), before_normalized_noop);
    }

    #[test]
    fn descriptor_failures_are_atomic_for_ids_layout_and_normalized_flags() {
        let chunk_type = ChunkType::new(0xbeef).unwrap();
        let mut document = Document::new_chunk();
        let id = document
            .push_raw(raw(
                chunk_type,
                ChunkFlags::CRITICAL,
                PayloadInput::Borrowed(b"opaque"),
                explicit_policy(ReservedBitsPolicy::Reject),
            ))
            .unwrap();
        document.dirty = false;
        let invalid = ChunkId::from_session_counter(99);
        let original = snapshot(&document);

        assert_eq!(
            document.set_type(invalid, ChunkType::META, RawChunkPolicy::infer()),
            Err(EditError::InvalidChunkId)
        );
        assert_eq!(
            document.set_flags(invalid, ChunkFlags::NONE, RawChunkPolicy::infer()),
            Err(EditError::InvalidChunkId)
        );
        assert_eq!(snapshot(&document), original);

        assert_eq!(
            document.set_flags(
                id,
                ChunkFlags::from_bits_retain(0xa500),
                RawChunkPolicy {
                    relocation: RelocationAssumption::Infer,
                    critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
                    reserved_flag_bits: ReservedBitsPolicy::Normalize,
                },
            ),
            Err(EditError::RelocationAssumptionRequired { chunk_type })
        );
        assert_eq!(snapshot(&document), original);

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
            flat.set_type(invalid, ChunkType::META, RawChunkPolicy::infer()),
            Err(EditError::ChunkLayoutRequired)
        );
        assert_eq!(
            flat.set_flags(invalid, ChunkFlags::NONE, RawChunkPolicy::infer()),
            Err(EditError::ChunkLayoutRequired)
        );
        assert!(!flat.is_dirty());
        assert_eq!(flat.flat_image().unwrap().main(), [7]);
    }

    #[test]
    fn descriptor_edits_keep_order_storage_allocations_and_container_state() {
        let mut source = encode_chunks(&[
            (ChunkType::META.raw(), 0, b"source-primary"),
            (ChunkType::FONT.raw(), 0, b"source-font"),
        ]);
        set_primary(&mut source, ChunkType::META);
        let borrowed = [1, 3, 3, 7];
        let owned = Vec::from([2, 4, 6, 8, 10]);
        let owned_pointer = owned.as_ptr() as usize;
        let owned_capacity = owned.capacity();
        let mut document = Document::from_vec(source).unwrap();
        let source_ids: Vec<_> = document.chunks().map(|chunk| chunk.id()).collect();
        let borrowed_id = document
            .push_raw(raw(
                ChunkType::PALETTE,
                ChunkFlags::NONE,
                PayloadInput::Borrowed(&borrowed),
                explicit_policy(ReservedBitsPolicy::Reject),
            ))
            .unwrap();
        let owned_id = document
            .push_raw(raw(
                ChunkType::new(0xbeef).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Owned(owned),
                explicit_policy(ReservedBitsPolicy::Reject),
            ))
            .unwrap();
        let ids: Vec<_> = document.chunks().map(|chunk| chunk.id()).collect();
        let storages: Vec<_> = ids
            .iter()
            .map(|id| storage_snapshot(&document, node(&document, *id)))
            .collect();
        let vector_pointer = nodes(&document).as_ptr() as usize;
        let vector_capacity = match &document.state {
            DocumentState::Chunk(chunks) => chunks.chunks.capacity(),
            _ => unreachable!(),
        };
        let primary = document.primary();
        document.next_id = u32::MAX;
        document.dirty = false;

        document
            .set_type(
                source_ids[0],
                ChunkType::new(0xcafe).unwrap(),
                explicit_policy(ReservedBitsPolicy::Reject),
            )
            .unwrap();
        document
            .set_flags(
                borrowed_id,
                ChunkFlags::CRITICAL,
                explicit_policy(ReservedBitsPolicy::Reject),
            )
            .unwrap();
        document
            .set_type(
                owned_id,
                ChunkType::new(0xffff).unwrap(),
                explicit_policy(ReservedBitsPolicy::Reject),
            )
            .unwrap();

        assert_eq!(
            document
                .chunks()
                .map(|chunk| chunk.id())
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(document.primary(), primary);
        assert_eq!(document.next_id, u32::MAX);
        assert_eq!(nodes(&document).as_ptr() as usize, vector_pointer);
        let capacity = match &document.state {
            DocumentState::Chunk(chunks) => chunks.chunks.capacity(),
            _ => unreachable!(),
        };
        assert_eq!(capacity, vector_capacity);
        for (id, expected) in ids.iter().zip(storages) {
            assert_eq!(storage_snapshot(&document, node(&document, *id)), expected);
        }
        assert_eq!(
            storage_snapshot(&document, node(&document, owned_id)).pointer,
            owned_pointer
        );
        assert_eq!(
            storage_snapshot(&document, node(&document, owned_id)).owned_capacity,
            Some(owned_capacity)
        );
        assert_eq!(
            document.get(source_ids[0]).unwrap().chunk_type(),
            ChunkType::new(0xcafe).unwrap()
        );
        assert_eq!(
            document.get(borrowed_id).unwrap().flags(),
            ChunkFlags::CRITICAL
        );
        assert_eq!(
            document.get(owned_id).unwrap().chunk_type(),
            ChunkType::new(0xffff).unwrap()
        );
        assert!(document.is_dirty());
    }
}
