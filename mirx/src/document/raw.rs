use alloc::vec::Vec;

use super::descriptor::{evaluate_descriptor, evaluate_descriptor_with_flags, evaluate_flags};
use super::{ChunkNode, Document, DocumentState, PayloadStorage, RewriteCapability};
use crate::{ChunkFlags, ChunkId, ChunkType, EditError};

/// Encoded payload bytes supplied to a raw document mutation.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PayloadInput<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl<'a> PayloadInput<'a> {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes.as_slice(),
        }
    }

    fn into_storage(self) -> PayloadStorage<'a> {
        match self {
            Self::Borrowed(bytes) => PayloadStorage::Borrowed(bytes),
            Self::Owned(bytes) => PayloadStorage::Owned(bytes),
        }
    }
}

/// Raw chunk fields supplied to an insertion operation.
#[derive(Debug, Eq, PartialEq)]
pub struct RawChunkInput<'a> {
    pub chunk_type: ChunkType,
    pub flags: ChunkFlags,
    pub payload: PayloadInput<'a>,
    pub policy: RawChunkPolicy,
}

/// Descriptor retained after a chunk is removed from a document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemovedChunkMeta {
    pub id: ChunkId,
    pub chunk_type: ChunkType,
    pub flags: ChunkFlags,
    pub was_primary: bool,
}

/// Whether raw payload bytes may move when the document is encoded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum RelocationAssumption {
    #[default]
    Infer,
    AssumeRelocatable,
}

/// Whether the caller understands an opaque critical payload contract.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum CriticalAssumption {
    #[default]
    Infer,
    AssumeCriticalUnderstood,
}

/// Handling of MIRX 1.0 chunk flag bits whose meaning is reserved.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReservedBitsPolicy {
    #[default]
    Reject,
    Preserve,
    Normalize,
}

/// Safety policy for accepting an encoded raw payload.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RawChunkPolicy {
    pub relocation: RelocationAssumption,
    pub critical_semantics: CriticalAssumption,
    pub reserved_flag_bits: ReservedBitsPolicy,
}

impl RawChunkPolicy {
    pub const fn infer() -> Self {
        Self {
            relocation: RelocationAssumption::Infer,
            critical_semantics: CriticalAssumption::Infer,
            reserved_flag_bits: ReservedBitsPolicy::Reject,
        }
    }
}

impl Default for RawChunkPolicy {
    fn default() -> Self {
        Self::infer()
    }
}

struct PreparedRaw<'a> {
    chunk_type: ChunkType,
    flags: ChunkFlags,
    payload: PayloadStorage<'a>,
    capability: RewriteCapability,
}

impl<'a> PreparedRaw<'a> {
    fn into_node(self, id: ChunkId) -> ChunkNode<'a> {
        ChunkNode {
            id,
            chunk_type: self.chunk_type,
            flags: self.flags,
            payload: self.payload,
            capability: self.capability,
        }
    }
}

#[derive(Clone, Copy)]
enum InsertPosition {
    End,
    Before(ChunkId),
    After(ChunkId),
}

impl<'a> Document<'a> {
    /// Appends one encoded chunk without copying its payload bytes.
    pub fn push_raw(&mut self, input: RawChunkInput<'a>) -> Result<ChunkId, EditError> {
        self.insert_raw_at(InsertPosition::End, input)
    }

    /// Inserts one encoded chunk immediately before `anchor`.
    pub fn insert_before(
        &mut self,
        anchor: ChunkId,
        input: RawChunkInput<'a>,
    ) -> Result<ChunkId, EditError> {
        self.insert_raw_at(InsertPosition::Before(anchor), input)
    }

    /// Inserts one encoded chunk immediately after `anchor`.
    pub fn insert_after(
        &mut self,
        anchor: ChunkId,
        input: RawChunkInput<'a>,
    ) -> Result<ChunkId, EditError> {
        self.insert_raw_at(InsertPosition::After(anchor), input)
    }

    /// Replaces the encoded payload of `id` without changing its descriptor.
    ///
    /// An exact byte match is a no-op and retains the existing storage and
    /// rewrite capability. Otherwise the replacement is checked with the same
    /// raw-payload policy used by insertion. Because this operation preserves
    /// the chunk descriptor, [`ReservedBitsPolicy::Normalize`] returns
    /// [`EditError::ReservedFlagBits`] when the existing flags contain reserved
    /// bits; use a descriptor edit to clear those bits.
    pub fn replace_raw(
        &mut self,
        id: ChunkId,
        payload: PayloadInput<'a>,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        let index = chunk_index(&self.state, id)?;
        let (chunk_type, flags, matches_existing) = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout checked before preparing replacement");
            };
            let node = &chunks.chunks[index];
            let existing = payload_bytes(&self.origin, node);
            (node.chunk_type, node.flags, existing == payload.as_bytes())
        };

        if matches_existing {
            return Ok(());
        }

        let prepared = prepare_replacement(chunk_type, flags, payload, policy)?;
        let DocumentState::Chunk(chunks) = &mut self.state else {
            unreachable!("layout checked before committing replacement");
        };
        let node = &mut chunks.chunks[index];
        node.payload = prepared.payload;
        node.capability = prepared.capability;
        self.dirty = true;
        Ok(())
    }

    /// Removes `id` without materializing its payload bytes.
    pub fn remove(&mut self, id: ChunkId) -> Result<RemovedChunkMeta, EditError> {
        let index = chunk_index(&self.state, id)?;
        let (_, meta) = self.remove_at(index);
        Ok(meta)
    }

    /// Removes `id` and returns an owned copy of its encoded payload.
    ///
    /// An already-owned payload is moved without changing its allocation.
    /// Borrowed and source-backed payloads are copied before the document is
    /// changed, so allocation failure leaves the document untouched.
    pub fn remove_to_vec(&mut self, id: ChunkId) -> Result<Vec<u8>, EditError> {
        self.remove_to_vec_with(id, copy_payload)
    }

    fn insert_raw_at(
        &mut self,
        position: InsertPosition,
        input: RawChunkInput<'a>,
    ) -> Result<ChunkId, EditError> {
        self.insert_raw_at_with(position, input, reserve_one_node)
    }

    fn insert_raw_at_with<R>(
        &mut self,
        position: InsertPosition,
        input: RawChunkInput<'a>,
        reserve: R,
    ) -> Result<ChunkId, EditError>
    where
        R: FnOnce(&mut Vec<ChunkNode<'a>>) -> Result<(), EditError>,
    {
        let index = insertion_index(&self.state, position)?;
        let prepared = prepare_raw(input)?;
        let following_id = self
            .next_id
            .checked_add(1)
            .ok_or(EditError::ChunkIdExhausted)?;
        let id = ChunkId::from_session_counter(self.next_id);
        let node = prepared.into_node(id);

        let DocumentState::Chunk(chunks) = &mut self.state else {
            unreachable!("layout checked before preparing insertion");
        };
        reserve(&mut chunks.chunks)?;
        chunks.chunks.insert(index, node);

        self.next_id = following_id;
        self.dirty = true;
        Ok(id)
    }

    fn remove_to_vec_with<C>(&mut self, id: ChunkId, copy: C) -> Result<Vec<u8>, EditError>
    where
        C: FnOnce(&[u8]) -> Result<Vec<u8>, EditError>,
    {
        let index = chunk_index(&self.state, id)?;
        let copied = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout checked before materializing removal");
            };
            match &chunks.chunks[index].payload {
                PayloadStorage::Owned(_) => None,
                PayloadStorage::Borrowed(bytes) => Some(copy(bytes)?),
                PayloadStorage::SourceRange(range) => {
                    let bytes = self
                        .origin
                        .resolve(*range)
                        .expect("validated source range must remain resolvable");
                    Some(copy(bytes)?)
                }
            }
        };

        let (node, _) = self.remove_at(index);
        match (copied, node.payload) {
            (Some(bytes), PayloadStorage::SourceRange(_) | PayloadStorage::Borrowed(_)) => {
                Ok(bytes)
            }
            (None, PayloadStorage::Owned(bytes)) => Ok(bytes),
            _ => unreachable!("payload storage cannot change during removal"),
        }
    }

    fn remove_at(&mut self, index: usize) -> (ChunkNode<'a>, RemovedChunkMeta) {
        let DocumentState::Chunk(chunks) = &mut self.state else {
            unreachable!("layout checked before committing removal");
        };
        let node = chunks.chunks.remove(index);
        let was_primary = chunks.primary == Some(node.id);
        if was_primary {
            chunks.primary = None;
        }
        let meta = RemovedChunkMeta {
            id: node.id,
            chunk_type: node.chunk_type,
            flags: node.flags,
            was_primary,
        };
        self.dirty = true;
        (node, meta)
    }
}

fn chunk_index(state: &DocumentState<'_>, id: ChunkId) -> Result<usize, EditError> {
    let DocumentState::Chunk(chunks) = state else {
        return Err(EditError::ChunkLayoutRequired);
    };
    chunks
        .chunks
        .iter()
        .position(|node| node.id == id)
        .ok_or(EditError::InvalidChunkId)
}

fn payload_bytes<'document>(
    origin: &'document super::source::Origin<'_>,
    node: &'document ChunkNode<'_>,
) -> &'document [u8] {
    match &node.payload {
        PayloadStorage::SourceRange(range) => origin
            .resolve(*range)
            .expect("validated source range must remain resolvable"),
        PayloadStorage::Borrowed(bytes) => bytes,
        PayloadStorage::Owned(bytes) => bytes.as_slice(),
    }
}

fn insertion_index(
    state: &DocumentState<'_>,
    position: InsertPosition,
) -> Result<usize, EditError> {
    let DocumentState::Chunk(chunks) = state else {
        return Err(EditError::ChunkLayoutRequired);
    };
    match position {
        InsertPosition::End => Ok(chunks.chunks.len()),
        InsertPosition::Before(anchor) => chunks
            .chunks
            .iter()
            .position(|node| node.id == anchor)
            .ok_or(EditError::InvalidChunkId),
        InsertPosition::After(anchor) => chunks
            .chunks
            .iter()
            .position(|node| node.id == anchor)
            .and_then(|index| index.checked_add(1))
            .ok_or(EditError::InvalidChunkId),
    }
}

fn prepare_raw(input: RawChunkInput<'_>) -> Result<PreparedRaw<'_>, EditError> {
    let descriptor = evaluate_descriptor(
        input.chunk_type,
        input.flags,
        input.payload.as_bytes(),
        input.policy,
    )?;

    Ok(PreparedRaw {
        chunk_type: descriptor.chunk_type,
        flags: descriptor.flags,
        payload: input.payload.into_storage(),
        capability: descriptor.capability,
    })
}

fn prepare_replacement(
    chunk_type: ChunkType,
    flags: ChunkFlags,
    payload: PayloadInput<'_>,
    policy: RawChunkPolicy,
) -> Result<PreparedRaw<'_>, EditError> {
    let evaluated_flags = evaluate_flags(flags, policy.reserved_flag_bits)?;
    if evaluated_flags.flags != flags {
        return Err(EditError::ReservedFlagBits {
            bits: flags.bits() & !ChunkFlags::CRITICAL.bits(),
        });
    }
    let descriptor =
        evaluate_descriptor_with_flags(chunk_type, evaluated_flags, payload.as_bytes(), policy)?;

    Ok(PreparedRaw {
        chunk_type: descriptor.chunk_type,
        flags: descriptor.flags,
        payload: payload.into_storage(),
        capability: descriptor.capability,
    })
}

fn copy_payload(payload: &[u8]) -> Result<Vec<u8>, EditError> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(payload.len())
        .map_err(|_| EditError::AllocationFailed)?;
    copy.extend_from_slice(payload);
    Ok(copy)
}

fn reserve_one_node(chunks: &mut Vec<ChunkNode<'_>>) -> Result<(), EditError> {
    chunks
        .try_reserve(1)
        .map_err(|_| EditError::AllocationFailed)
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::header::{CHUNK_FILE_HEADER_LEN, chunk_type};
    use crate::{
        ColorFormat, FlatImageInput, ImageChunkInput, PayloadOrigin, encode_chunk_image,
        encode_chunks, encode_flat,
    };

    const fn assumed_policy() -> RawChunkPolicy {
        RawChunkPolicy {
            relocation: RelocationAssumption::AssumeRelocatable,
            critical_semantics: CriticalAssumption::Infer,
            reserved_flag_bits: ReservedBitsPolicy::Reject,
        }
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

    fn ids(document: &Document<'_>) -> Vec<ChunkId> {
        document.chunks().map(|chunk| chunk.id()).collect()
    }

    fn types(document: &Document<'_>) -> Vec<ChunkType> {
        document.chunks().map(|chunk| chunk.chunk_type()).collect()
    }

    fn set_primary(source: &mut [u8], chunk_type: ChunkType) {
        source[20..22].copy_from_slice(&chunk_type.raw().to_le_bytes());
        let checksum = crate::crc32(&source[..40]);
        source[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    #[test]
    fn borrowed_and_owned_inputs_retain_allocations_and_provenance() {
        let borrowed = [10, 20, 30];
        let borrowed_pointer = borrowed.as_ptr();
        let owned = vec![40, 50, 60, 70];
        let owned_pointer = owned.as_ptr();
        let owned_len = owned.len();
        let owned_capacity = owned.capacity();
        let mut document = Document::new_chunk();

        let borrowed_id = document
            .push_raw(raw(
                ChunkType::new(1).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Borrowed(&borrowed),
                assumed_policy(),
            ))
            .unwrap();
        let owned_id = document
            .push_raw(raw(
                ChunkType::new(0xffff).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Owned(owned),
                assumed_policy(),
            ))
            .unwrap();

        let borrowed_view = document.get(borrowed_id).unwrap();
        assert_eq!(borrowed_view.payload_origin(), PayloadOrigin::BORROWED);
        assert_eq!(
            borrowed_view.payload_bytes().unwrap().as_ptr(),
            borrowed_pointer
        );
        assert_eq!(borrowed_view.payload_len(), Ok(borrowed.len()));

        let owned_view = document.get(owned_id).unwrap();
        assert_eq!(owned_view.payload_origin(), PayloadOrigin::OWNED);
        assert_eq!(owned_view.payload_bytes().unwrap().as_ptr(), owned_pointer);
        assert_eq!(owned_view.payload_len(), Ok(owned_len));
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("expected CHUNK document");
        };
        let PayloadStorage::Owned(stored) = &chunks.chunks[1].payload else {
            panic!("expected owned payload");
        };
        assert_eq!(stored.capacity(), owned_capacity);
    }

    #[test]
    fn opened_payloads_keep_original_source_provenance_after_insertions() {
        let source = encode_chunks(&[(chunk_type::META, 0, b"source")]);
        let source_pointer = {
            let entry = CHUNK_FILE_HEADER_LEN;
            let start = u32::from_le_bytes(source[entry + 4..entry + 8].try_into().unwrap());
            source[start as usize..].as_ptr()
        };
        let mut document = Document::open(&source).unwrap();
        let source_id = document.chunks().next().unwrap().id();
        document
            .push_raw(raw(
                ChunkType::new(0xbeef).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"added"),
                assumed_policy(),
            ))
            .unwrap();

        let source_view = document.get(source_id).unwrap();
        assert_eq!(source_view.payload_origin(), PayloadOrigin::ORIGINAL_SOURCE);
        assert_eq!(
            source_view.payload_bytes().unwrap().as_ptr(),
            source_pointer
        );
    }

    #[test]
    fn owned_source_can_accept_a_later_borrowed_payload() {
        let source = encode_chunks(&[(chunk_type::META, 0, b"source")]);
        let added = vec![8, 9, 10];
        let added_pointer = added.as_ptr();
        let mut document = Document::from_vec(source).unwrap();
        let id = document
            .push_raw(raw(
                ChunkType::new(0xbeef).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Borrowed(&added),
                assumed_policy(),
            ))
            .unwrap();

        assert_eq!(
            document.get(id).unwrap().payload_bytes().unwrap().as_ptr(),
            added_pointer
        );
        assert_eq!(
            document.get(id).unwrap().payload_origin(),
            PayloadOrigin::BORROWED
        );
    }

    #[test]
    fn positional_insertions_preserve_exact_order_duplicates_and_existing_ids() {
        let source = encode_chunks(&[(chunk_type::META, 0, b"a"), (chunk_type::FONT, 0, b"b")]);
        let mut document = Document::open(&source).unwrap();
        let original_ids = ids(&document);

        let pushed = document
            .push_raw(raw(
                ChunkType::FONT,
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"tail"),
                assumed_policy(),
            ))
            .unwrap();
        let before = document
            .insert_before(
                original_ids[0],
                raw(
                    ChunkType::new(1).unwrap(),
                    ChunkFlags::NONE,
                    PayloadInput::Borrowed(b"first"),
                    assumed_policy(),
                ),
            )
            .unwrap();
        let after = document
            .insert_after(
                original_ids[0],
                raw(
                    ChunkType::FONT,
                    ChunkFlags::NONE,
                    PayloadInput::Borrowed(b"middle"),
                    assumed_policy(),
                ),
            )
            .unwrap();

        assert_eq!(
            ids(&document),
            [before, original_ids[0], after, original_ids[1], pushed]
        );
        assert_eq!(
            types(&document),
            [
                ChunkType::new(1).unwrap(),
                ChunkType::META,
                ChunkType::FONT,
                ChunkType::FONT,
                ChunkType::FONT,
            ]
        );
        assert_eq!(
            document.get(original_ids[0]).unwrap().payload_bytes(),
            Some(b"a".as_slice())
        );
        assert_eq!(
            document.get(original_ids[1]).unwrap().payload_bytes(),
            Some(b"b".as_slice())
        );
    }

    #[test]
    fn opened_and_new_documents_assign_monotonic_ids_and_become_dirty() {
        let source = encode_chunks(&[(chunk_type::META, 0, b"a"), (chunk_type::FONT, 0, b"b")]);
        let mut opened = Document::open(&source).unwrap();
        assert!(!opened.is_dirty());
        let first = opened
            .push_raw(raw(
                ChunkType::new(0xbeef).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"c"),
                assumed_policy(),
            ))
            .unwrap();
        let second = opened
            .push_raw(raw(
                ChunkType::new(0xbeef).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"d"),
                assumed_policy(),
            ))
            .unwrap();
        assert_eq!(first, ChunkId::from_session_counter(2));
        assert_eq!(second, ChunkId::from_session_counter(3));
        assert!(opened.is_dirty());
        assert_eq!(opened.next_id, 4);

        let mut new = Document::new_chunk();
        let id = new
            .push_raw(raw(
                ChunkType::new(1).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b""),
                assumed_policy(),
            ))
            .unwrap();
        assert_eq!(id, ChunkId::from_session_counter(0));
        assert!(new.is_dirty());
        assert_eq!(new.get(id).unwrap().payload_bytes(), Some(b"".as_slice()));
    }

    #[test]
    fn raw_type_boundaries_and_flags_are_retained() {
        let mut document = Document::new_chunk();
        for (raw_type, flag_bits) in [(1, 0), (0xbeef, 1), (0xffff, 0xa500)] {
            let reserved_flag_bits = if flag_bits & !ChunkFlags::CRITICAL.bits() == 0 {
                ReservedBitsPolicy::Reject
            } else {
                ReservedBitsPolicy::Preserve
            };
            let id = document
                .push_raw(raw(
                    ChunkType::new(raw_type).unwrap(),
                    ChunkFlags::from_bits_retain(flag_bits),
                    PayloadInput::Borrowed(b"opaque"),
                    RawChunkPolicy {
                        relocation: RelocationAssumption::AssumeRelocatable,
                        critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
                        reserved_flag_bits,
                    },
                ))
                .unwrap();
            let chunk = document.get(id).unwrap();
            assert_eq!(chunk.chunk_type().raw(), raw_type);
            assert_eq!(chunk.flags().bits(), flag_bits);
        }
    }

    #[test]
    fn opaque_payloads_require_explicit_relocation_and_critical_assumptions() {
        let custom = ChunkType::new(0xbeef).unwrap();
        let source = encode_chunks(&[]);
        let mut document = Document::open(&source).unwrap();
        assert!(!document.is_dirty());
        assert_eq!(
            document.push_raw(raw(
                custom,
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"opaque"),
                RawChunkPolicy::infer(),
            )),
            Err(EditError::RelocationAssumptionRequired { chunk_type: custom })
        );
        assert_eq!(document.next_id, 0);
        assert!(document.chunks().next().is_none());
        assert!(!document.is_dirty());

        assert_eq!(
            document.push_raw(raw(
                custom,
                ChunkFlags::CRITICAL,
                PayloadInput::Borrowed(b"opaque"),
                assumed_policy(),
            )),
            Err(EditError::CriticalAssumptionRequired { chunk_type: custom })
        );
        assert_eq!(document.next_id, 0);
        assert!(!document.is_dirty());

        let id = document
            .push_raw(raw(
                custom,
                ChunkFlags::CRITICAL,
                PayloadInput::Borrowed(b"opaque"),
                RawChunkPolicy {
                    relocation: RelocationAssumption::AssumeRelocatable,
                    critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
                    reserved_flag_bits: ReservedBitsPolicy::Reject,
                },
            ))
            .unwrap();
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("expected CHUNK document");
        };
        let node = chunks.chunks.iter().find(|node| node.id == id).unwrap();
        assert!(node.capability.is_relocatable());
        assert!(node.capability.critical_understood());
    }

    #[test]
    fn reserved_flag_bits_reject_preserve_or_normalize_explicitly() {
        let custom = ChunkType::new(0xbeef).unwrap();
        let flags = ChunkFlags::from_bits_retain(0xa501);
        let mut document = Document::new_chunk();
        assert_eq!(
            document.push_raw(raw(
                custom,
                flags,
                PayloadInput::Borrowed(b"opaque"),
                RawChunkPolicy {
                    relocation: RelocationAssumption::AssumeRelocatable,
                    critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
                    reserved_flag_bits: ReservedBitsPolicy::Reject,
                },
            )),
            Err(EditError::ReservedFlagBits { bits: 0xa500 })
        );

        let preserved = document
            .push_raw(raw(
                custom,
                flags,
                PayloadInput::Borrowed(b"preserve"),
                RawChunkPolicy {
                    relocation: RelocationAssumption::AssumeRelocatable,
                    critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
                    reserved_flag_bits: ReservedBitsPolicy::Preserve,
                },
            ))
            .unwrap();
        let normalized = document
            .push_raw(raw(
                custom,
                flags,
                PayloadInput::Borrowed(b"normalize"),
                RawChunkPolicy {
                    relocation: RelocationAssumption::AssumeRelocatable,
                    critical_semantics: CriticalAssumption::AssumeCriticalUnderstood,
                    reserved_flag_bits: ReservedBitsPolicy::Normalize,
                },
            ))
            .unwrap();
        assert_eq!(document.get(preserved).unwrap().flags().bits(), 0xa501);
        assert_eq!(
            document.get(normalized).unwrap().flags(),
            ChunkFlags::CRITICAL
        );
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("expected CHUNK document");
        };
        let preserved_node = chunks
            .chunks
            .iter()
            .find(|node| node.id == preserved)
            .unwrap();
        let normalized_node = chunks
            .chunks
            .iter()
            .find(|node| node.id == normalized)
            .unwrap();
        assert!(preserved_node.capability.preserves_reserved_bits());
        assert!(!normalized_node.capability.preserves_reserved_bits());
    }

    #[test]
    fn image_payload_infers_contract_and_rejects_malformed_bytes() {
        let payload = valid_image_payload();
        let pointer = payload.as_ptr();
        let mut document = Document::new_chunk();
        let id = document
            .push_raw(raw(
                ChunkType::IMAGE,
                ChunkFlags::CRITICAL,
                PayloadInput::Owned(payload),
                RawChunkPolicy::infer(),
            ))
            .unwrap();
        assert_eq!(
            document.get(id).unwrap().payload_bytes().unwrap().as_ptr(),
            pointer
        );
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("expected CHUNK document");
        };
        assert!(chunks.chunks[0].capability.is_relocatable());
        assert!(chunks.chunks[0].capability.critical_understood());

        let before_ids = ids(&document);
        let before_next = document.next_id;
        let error = document
            .push_raw(raw(
                ChunkType::IMAGE,
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"short"),
                RawChunkPolicy::infer(),
            ))
            .unwrap_err();
        assert!(matches!(error, EditError::InvalidPayload(_)));
        assert_eq!(ids(&document), before_ids);
        assert_eq!(document.next_id, before_next);

        let opaque = document
            .push_raw(raw(
                ChunkType::IMAGE,
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"opaque image contract"),
                assumed_policy(),
            ))
            .unwrap();
        assert_eq!(
            document.get(opaque).unwrap().payload_bytes(),
            Some(b"opaque image contract".as_slice())
        );
        assert_eq!(
            document.push_raw(raw(
                ChunkType::IMAGE,
                ChunkFlags::CRITICAL,
                PayloadInput::Borrowed(b"opaque critical image contract"),
                assumed_policy(),
            )),
            Err(EditError::CriticalAssumptionRequired {
                chunk_type: ChunkType::IMAGE,
            })
        );
    }

    #[test]
    fn replacement_preserves_descriptor_identity_order_primary_and_id_counter() {
        let mut source = encode_chunks(&[
            (chunk_type::META, 0, b"old"),
            (chunk_type::FONT, 0xa500, b"font"),
        ]);
        set_primary(&mut source, ChunkType::META);
        let replacement = [9, 8, 7, 6];
        let replacement_pointer = replacement.as_ptr();
        let mut document = Document::open(&source).unwrap();
        let original_ids = ids(&document);
        let original_primary = document.primary();
        let original_next = document.next_id;

        document
            .replace_raw(
                original_ids[0],
                PayloadInput::Borrowed(&replacement),
                assumed_policy(),
            )
            .unwrap();

        assert_eq!(ids(&document), original_ids);
        assert_eq!(document.primary(), original_primary);
        assert_eq!(document.next_id, original_next);
        let replaced = document.get(original_ids[0]).unwrap();
        assert_eq!(replaced.chunk_type(), ChunkType::META);
        assert_eq!(replaced.flags(), ChunkFlags::NONE);
        assert_eq!(replaced.payload_origin(), PayloadOrigin::BORROWED);
        assert_eq!(
            replaced.payload_bytes().unwrap().as_ptr(),
            replacement_pointer
        );
        let untouched = document.get(original_ids[1]).unwrap();
        assert_eq!(untouched.chunk_type(), ChunkType::FONT);
        assert_eq!(untouched.flags(), ChunkFlags::from_bits_retain(0xa500));
        assert_eq!(untouched.payload_bytes(), Some(b"font".as_slice()));
        assert!(document.is_dirty());
    }

    #[test]
    fn replacement_moves_owned_allocation_and_exact_match_keeps_existing_storage() {
        let source = encode_chunks(&[(chunk_type::META, 0, b"same")]);
        let source_payload_pointer = {
            let entry = CHUNK_FILE_HEADER_LEN;
            let start = u32::from_le_bytes(source[entry + 4..entry + 8].try_into().unwrap());
            source[start as usize..].as_ptr()
        };
        let mut document = Document::open(&source).unwrap();
        let id = document.chunks().next().unwrap().id();

        document
            .replace_raw(
                id,
                PayloadInput::Owned(b"same".to_vec()),
                RawChunkPolicy::infer(),
            )
            .unwrap();
        let unchanged = document.get(id).unwrap();
        assert_eq!(unchanged.payload_origin(), PayloadOrigin::ORIGINAL_SOURCE);
        assert_eq!(
            unchanged.payload_bytes().unwrap().as_ptr(),
            source_payload_pointer
        );
        assert!(!document.is_dirty());
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("expected CHUNK document");
        };
        assert_eq!(
            chunks.chunks[0].capability,
            RewriteCapability::PRESERVE_ONLY
        );

        let owned = vec![1, 2, 3, 4, 5];
        let owned_pointer = owned.as_ptr();
        let owned_len = owned.len();
        let owned_capacity = owned.capacity();
        document
            .replace_raw(id, PayloadInput::Owned(owned), assumed_policy())
            .unwrap();
        let replaced = document.get(id).unwrap();
        assert_eq!(replaced.payload_origin(), PayloadOrigin::OWNED);
        assert_eq!(replaced.payload_bytes().unwrap().as_ptr(), owned_pointer);
        assert_eq!(replaced.payload_len(), Ok(owned_len));
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("expected CHUNK document");
        };
        let PayloadStorage::Owned(stored) = &chunks.chunks[0].payload else {
            panic!("expected owned payload");
        };
        assert_eq!(stored.capacity(), owned_capacity);
        let capability = chunks.chunks[0].capability;

        document.dirty = false;
        document
            .replace_raw(
                id,
                PayloadInput::Borrowed(&[1, 2, 3, 4, 5]),
                RawChunkPolicy::infer(),
            )
            .unwrap();
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("expected CHUNK document");
        };
        let PayloadStorage::Owned(stored) = &chunks.chunks[0].payload else {
            panic!("exact replacement must retain owned storage");
        };
        assert_eq!(stored.as_ptr(), owned_pointer);
        assert_eq!(stored.capacity(), owned_capacity);
        assert_eq!(chunks.chunks[0].capability, capability);
        assert!(!document.is_dirty());
    }

    #[test]
    fn replacement_policy_reserved_bits_and_image_validation_are_failure_atomic() {
        let reserved_flags = ChunkFlags::from_bits_retain(0xa500);
        let custom = ChunkType::new(0xbeef).unwrap();
        let mut document = Document::new_chunk();
        let reserved_id = document
            .push_raw(raw(
                custom,
                reserved_flags,
                PayloadInput::Borrowed(b"old"),
                RawChunkPolicy {
                    relocation: RelocationAssumption::AssumeRelocatable,
                    critical_semantics: CriticalAssumption::Infer,
                    reserved_flag_bits: ReservedBitsPolicy::Preserve,
                },
            ))
            .unwrap();
        document.dirty = false;
        let before_next = document.next_id;

        document
            .replace_raw(
                reserved_id,
                PayloadInput::Borrowed(b"old"),
                RawChunkPolicy::infer(),
            )
            .unwrap();
        assert!(!document.is_dirty());

        for reserved_flag_bits in [ReservedBitsPolicy::Reject, ReservedBitsPolicy::Normalize] {
            assert_eq!(
                document.replace_raw(
                    reserved_id,
                    PayloadInput::Borrowed(b"new"),
                    RawChunkPolicy {
                        relocation: RelocationAssumption::AssumeRelocatable,
                        critical_semantics: CriticalAssumption::Infer,
                        reserved_flag_bits,
                    },
                ),
                Err(EditError::ReservedFlagBits { bits: 0xa500 })
            );
            assert_eq!(
                document.get(reserved_id).unwrap().payload_bytes(),
                Some(b"old".as_slice())
            );
            assert_eq!(document.get(reserved_id).unwrap().flags(), reserved_flags);
            assert_eq!(document.next_id, before_next);
            assert!(!document.is_dirty());
        }

        document
            .replace_raw(
                reserved_id,
                PayloadInput::Borrowed(b"new"),
                RawChunkPolicy {
                    relocation: RelocationAssumption::AssumeRelocatable,
                    critical_semantics: CriticalAssumption::Infer,
                    reserved_flag_bits: ReservedBitsPolicy::Preserve,
                },
            )
            .unwrap();
        assert_eq!(document.get(reserved_id).unwrap().flags(), reserved_flags);
        let DocumentState::Chunk(chunks) = &document.state else {
            panic!("expected CHUNK document");
        };
        assert!(chunks.chunks[0].capability.preserves_reserved_bits());

        let image_payload = valid_image_payload();
        let image_id = document
            .push_raw(raw(
                ChunkType::IMAGE,
                ChunkFlags::NONE,
                PayloadInput::Owned(image_payload),
                RawChunkPolicy::infer(),
            ))
            .unwrap();
        document.dirty = false;
        let before = document
            .get(image_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .to_vec();
        let before_pointer = document
            .get(image_id)
            .unwrap()
            .payload_bytes()
            .unwrap()
            .as_ptr();
        let before_next = document.next_id;
        assert!(matches!(
            document.replace_raw(
                image_id,
                PayloadInput::Borrowed(b"invalid"),
                RawChunkPolicy::infer(),
            ),
            Err(EditError::InvalidPayload(_))
        ));
        let image = document.get(image_id).unwrap();
        assert_eq!(image.payload_bytes(), Some(before.as_slice()));
        assert_eq!(image.payload_bytes().unwrap().as_ptr(), before_pointer);
        assert_eq!(document.next_id, before_next);
        assert!(!document.is_dirty());

        let source = encode_chunks(&[(chunk_type::META, 0, b"opaque")]);
        let mut opened = Document::open(&source).unwrap();
        let id = opened.chunks().next().unwrap().id();
        assert_eq!(
            opened.replace_raw(
                id,
                PayloadInput::Borrowed(b"changed"),
                RawChunkPolicy::infer(),
            ),
            Err(EditError::RelocationAssumptionRequired {
                chunk_type: ChunkType::META,
            })
        );
        assert_eq!(
            opened.get(id).unwrap().payload_bytes(),
            Some(b"opaque".as_slice())
        );
        assert!(!opened.is_dirty());
    }

    #[test]
    fn removal_returns_descriptor_clears_only_removed_primary_and_keeps_ids_monotonic() {
        let mut source = encode_chunks(&[
            (chunk_type::META, 0xa500, b"meta"),
            (chunk_type::FONT, 0, b"primary"),
            (chunk_type::FONT, 0, b"same-type successor"),
        ]);
        set_primary(&mut source, ChunkType::FONT);
        let mut document = Document::open(&source).unwrap();
        let original_ids = ids(&document);
        let original_next = document.next_id;

        let non_primary = document.remove(original_ids[0]).unwrap();
        assert_eq!(
            non_primary,
            RemovedChunkMeta {
                id: original_ids[0],
                chunk_type: ChunkType::META,
                flags: ChunkFlags::from_bits_retain(0xa500),
                was_primary: false,
            }
        );
        assert_eq!(document.primary(), Some(original_ids[1]));
        assert_eq!(ids(&document), [original_ids[1], original_ids[2]]);
        assert_eq!(document.next_id, original_next);
        assert!(document.is_dirty());

        document.dirty = false;
        let primary = document.remove(original_ids[1]).unwrap();
        assert!(primary.was_primary);
        assert_eq!(primary.id, original_ids[1]);
        assert_eq!(primary.chunk_type, ChunkType::FONT);
        assert_eq!(primary.flags, ChunkFlags::NONE);
        assert_eq!(document.primary(), None);
        assert_eq!(ids(&document), [original_ids[2]]);
        assert_eq!(
            document.get(original_ids[2]).unwrap().chunk_type(),
            ChunkType::FONT
        );
        assert_eq!(document.next_id, original_next);
        assert!(document.is_dirty());

        let new_id = document
            .push_raw(raw(
                ChunkType::new(0xbeef).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"new"),
                assumed_policy(),
            ))
            .unwrap();
        assert_eq!(new_id, ChunkId::from_session_counter(original_next));
        assert_eq!(ids(&document), [original_ids[2], new_id]);
    }

    #[test]
    fn remove_to_vec_copies_borrowed_and_source_ranges_but_moves_owned_allocation() {
        let borrowed = [1, 2, 3, 4];
        let borrowed_pointer = borrowed.as_ptr();
        let owned = vec![5, 6, 7, 8, 9];
        let owned_pointer = owned.as_ptr();
        let owned_len = owned.len();
        let owned_capacity = owned.capacity();
        let mut document = Document::new_chunk();
        let borrowed_id = document
            .push_raw(raw(
                ChunkType::META,
                ChunkFlags::NONE,
                PayloadInput::Borrowed(&borrowed),
                assumed_policy(),
            ))
            .unwrap();
        let owned_id = document
            .push_raw(raw(
                ChunkType::FONT,
                ChunkFlags::NONE,
                PayloadInput::Owned(owned),
                assumed_policy(),
            ))
            .unwrap();

        let borrowed_result = document.remove_to_vec(borrowed_id).unwrap();
        assert_eq!(borrowed_result, borrowed);
        assert_ne!(borrowed_result.as_ptr(), borrowed_pointer);

        let owned_result = document.remove_to_vec(owned_id).unwrap();
        assert_eq!(owned_result, [5, 6, 7, 8, 9]);
        assert_eq!(owned_result.as_ptr(), owned_pointer);
        assert_eq!(owned_result.len(), owned_len);
        assert_eq!(owned_result.capacity(), owned_capacity);

        let mut source = encode_chunks(&[(chunk_type::PALETTE, 0, b"source payload")]);
        set_primary(&mut source, ChunkType::PALETTE);
        let source_pointer = {
            let entry = CHUNK_FILE_HEADER_LEN;
            let start = u32::from_le_bytes(source[entry + 4..entry + 8].try_into().unwrap());
            source[start as usize..].as_ptr()
        };
        let mut owned_source = Document::from_vec(source).unwrap();
        let source_id = owned_source.chunks().next().unwrap().id();
        let source_next = owned_source.next_id;
        let source_result = owned_source.remove_to_vec(source_id).unwrap();
        assert_eq!(source_result, b"source payload");
        assert_ne!(source_result.as_ptr(), source_pointer);
        assert_eq!(owned_source.chunks().len(), 0);
        assert_eq!(owned_source.primary(), None);
        assert_eq!(owned_source.next_id, source_next);
        assert!(owned_source.is_dirty());

        let empty = [];
        let empty_id = owned_source
            .push_raw(raw(
                ChunkType::META,
                ChunkFlags::NONE,
                PayloadInput::Borrowed(&empty),
                assumed_policy(),
            ))
            .unwrap();
        assert!(owned_source.remove_to_vec(empty_id).unwrap().is_empty());
    }

    #[test]
    fn replacement_and_removal_layout_id_and_copy_failures_are_atomic() {
        let flat_source = encode_flat(&FlatImageInput {
            width: 1,
            height: 1,
            stride: 1,
            format: ColorFormat::A8,
            main: &[7],
            extra: None,
        });
        let mut flat = Document::open(&flat_source).unwrap();
        let invalid = ChunkId::from_session_counter(99);
        assert_eq!(
            flat.replace_raw(
                invalid,
                PayloadInput::Borrowed(b"x"),
                RawChunkPolicy::infer(),
            ),
            Err(EditError::ChunkLayoutRequired)
        );
        assert_eq!(flat.remove(invalid), Err(EditError::ChunkLayoutRequired));
        assert_eq!(
            flat.remove_to_vec(invalid),
            Err(EditError::ChunkLayoutRequired)
        );
        assert!(!flat.is_dirty());

        let mut source = encode_chunks(&[(chunk_type::META, 0, b"source")]);
        set_primary(&mut source, ChunkType::META);
        let mut document = Document::open(&source).unwrap();
        let id = document.chunks().next().unwrap().id();
        assert_eq!(document.remove(invalid), Err(EditError::InvalidChunkId));
        assert_eq!(
            document.remove_to_vec(invalid),
            Err(EditError::InvalidChunkId)
        );
        assert_eq!(document.primary(), Some(id));
        assert!(!document.is_dirty());

        let before_ids = ids(&document);
        let before_primary = document.primary();
        let before_next = document.next_id;
        let before_bytes = document.get(id).unwrap().payload_bytes().unwrap().to_vec();
        let result = document.remove_to_vec_with(id, |_| Err(EditError::AllocationFailed));
        assert_eq!(result, Err(EditError::AllocationFailed));
        assert_eq!(ids(&document), before_ids);
        assert_eq!(document.primary(), before_primary);
        assert_eq!(document.next_id, before_next);
        assert_eq!(
            document.get(id).unwrap().payload_bytes(),
            Some(before_bytes.as_slice())
        );
        assert!(!document.is_dirty());

        let borrowed = [4, 5, 6];
        let mut borrowed_document = Document::new_chunk();
        let borrowed_id = borrowed_document
            .push_raw(raw(
                ChunkType::META,
                ChunkFlags::NONE,
                PayloadInput::Borrowed(&borrowed),
                assumed_policy(),
            ))
            .unwrap();
        borrowed_document.dirty = false;
        let borrowed_next = borrowed_document.next_id;
        let result =
            borrowed_document.remove_to_vec_with(borrowed_id, |_| Err(EditError::AllocationFailed));
        assert_eq!(result, Err(EditError::AllocationFailed));
        assert_eq!(ids(&borrowed_document), [borrowed_id]);
        assert_eq!(borrowed_document.next_id, borrowed_next);
        assert_eq!(
            borrowed_document.get(borrowed_id).unwrap().payload_bytes(),
            Some(borrowed.as_slice())
        );
        assert!(!borrowed_document.is_dirty());

        let owned = vec![10, 20, 30];
        let owned_pointer = owned.as_ptr();
        let owned_capacity = owned.capacity();
        let owned_id = document
            .push_raw(raw(
                ChunkType::new(0xbeef).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Owned(owned),
                assumed_policy(),
            ))
            .unwrap();
        let moved = document
            .remove_to_vec_with(owned_id, |_| panic!("owned payload must not be copied"))
            .unwrap();
        assert_eq!(moved.as_ptr(), owned_pointer);
        assert_eq!(moved.capacity(), owned_capacity);
    }

    #[test]
    fn invalid_layout_anchor_id_exhaustion_and_reserve_are_failure_atomic() {
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
            flat.push_raw(raw(
                ChunkType::new(0xbeef).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"opaque"),
                assumed_policy(),
            )),
            Err(EditError::ChunkLayoutRequired)
        );
        assert!(!flat.is_dirty());

        let mut document = Document::new_chunk();
        let first = document
            .push_raw(raw(
                ChunkType::new(0xbeef).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Borrowed(b"first"),
                assumed_policy(),
            ))
            .unwrap();
        document.dirty = false;
        let before_ids = ids(&document);
        let before_next = document.next_id;
        let invalid = ChunkId::from_session_counter(99);
        assert_eq!(
            document.insert_before(
                invalid,
                raw(
                    ChunkType::new(0xbeef).unwrap(),
                    ChunkFlags::NONE,
                    PayloadInput::Borrowed(b"bad"),
                    assumed_policy(),
                ),
            ),
            Err(EditError::InvalidChunkId)
        );
        assert_eq!(ids(&document), before_ids);
        assert_eq!(document.next_id, before_next);
        assert!(!document.is_dirty());

        document.next_id = u32::MAX;
        assert_eq!(
            document.insert_after(
                first,
                raw(
                    ChunkType::new(0xbeef).unwrap(),
                    ChunkFlags::NONE,
                    PayloadInput::Borrowed(b"exhausted"),
                    assumed_policy(),
                ),
            ),
            Err(EditError::ChunkIdExhausted)
        );
        assert_eq!(ids(&document), before_ids);
        assert_eq!(document.next_id, u32::MAX);
        assert!(!document.is_dirty());

        document.next_id = before_next;
        let result = document.insert_raw_at_with(
            InsertPosition::End,
            raw(
                ChunkType::new(0xbeef).unwrap(),
                ChunkFlags::NONE,
                PayloadInput::Owned(vec![1, 2, 3]),
                assumed_policy(),
            ),
            |_| Err(EditError::AllocationFailed),
        );
        assert_eq!(result, Err(EditError::AllocationFailed));
        assert_eq!(ids(&document), before_ids);
        assert_eq!(document.next_id, before_next);
        assert!(!document.is_dirty());
    }

    #[test]
    fn input_policy_defaults_are_conservative_and_node_size_stays_bounded() {
        assert_eq!(RawChunkPolicy::default(), RawChunkPolicy::infer());
        assert_eq!(RelocationAssumption::default(), RelocationAssumption::Infer);
        assert_eq!(CriticalAssumption::default(), CriticalAssumption::Infer);
        assert_eq!(ReservedBitsPolicy::default(), ReservedBitsPolicy::Reject);

        let size = core::mem::size_of::<ChunkNode<'_>>();
        match core::mem::size_of::<usize>() {
            4 => assert!(size <= 32),
            8 => assert!(size <= 48),
            _ => panic!("unsupported pointer width"),
        }
    }
}
