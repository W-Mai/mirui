use alloc::vec::Vec;

use super::descriptor::EvaluatedFlags;
use super::payload::{PayloadPlacement, ResolvedNodePayload, resolve_node_payload};
use super::primary::{PrimaryProjection, changed_primary_hint_state, ensure_primary_projection};
use super::{
    ChunkNode, ChunkSet, Document, DocumentState, EditError, EncodeError, PayloadStorage,
    PrimaryHintState, RewriteCapability, promoted_chunk_set, promoted_flat_node,
};
use crate::payload::image::ImagePayloadError;
use crate::{ChunkFlags, ChunkId, ChunkType};

/// Encoded payload bytes supplied to a raw document mutation.
#[derive(Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum PayloadInput<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl<'a> From<&'a [u8]> for PayloadInput<'a> {
    fn from(bytes: &'a [u8]) -> Self {
        Self::Borrowed(bytes)
    }
}

impl<'a, const N: usize> From<&'a [u8; N]> for PayloadInput<'a> {
    fn from(bytes: &'a [u8; N]) -> Self {
        Self::Borrowed(bytes)
    }
}

impl<'a> From<Vec<u8>> for PayloadInput<'a> {
    fn from(bytes: Vec<u8>) -> Self {
        Self::Owned(bytes)
    }
}

impl<'a> PayloadInput<'a> {
    pub(crate) fn as_bytes(&self) -> &[u8] {
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
pub(crate) struct RawChunkInput<'a> {
    pub(crate) chunk_type: ChunkType,
    pub(crate) flags: ChunkFlags,
    pub(crate) payload: PayloadInput<'a>,
    pub(crate) policy: RawChunkPolicy,
}

impl<'a> RawChunkInput<'a> {
    /// Creates a raw chunk input with no flags and conservative inference.
    pub(crate) fn new(chunk_type: ChunkType, payload: impl Into<PayloadInput<'a>>) -> Self {
        Self {
            chunk_type,
            flags: ChunkFlags::NONE,
            payload: payload.into(),
            policy: RawChunkPolicy::infer(),
        }
    }

    #[cfg(test)]
    pub(crate) const fn with_flags(mut self, flags: ChunkFlags) -> Self {
        self.flags = flags;
        self
    }

    #[cfg(test)]
    pub(crate) const fn with_policy(mut self, policy: RawChunkPolicy) -> Self {
        self.policy = policy;
        self
    }
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

impl ReservedBitsPolicy {
    pub(super) fn apply(self, flags: ChunkFlags) -> Result<EvaluatedFlags, EditError> {
        let reserved_bits = flags.bits() & !ChunkFlags::CRITICAL.bits();
        let (flags, preserve_reserved_bits) = match (reserved_bits, self) {
            (0, _) => (flags, false),
            (_, Self::Preserve) => (flags, true),
            (_, Self::Normalize) => (
                ChunkFlags::from_bits_retain(flags.bits() & ChunkFlags::CRITICAL.bits()),
                false,
            ),
            (_, Self::Reject) => {
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

    pub const fn with_relocation(mut self, relocation: RelocationAssumption) -> Self {
        self.relocation = relocation;
        self
    }

    pub const fn with_critical_semantics(mut self, critical_semantics: CriticalAssumption) -> Self {
        self.critical_semantics = critical_semantics;
        self
    }

    pub const fn with_reserved_bits(mut self, reserved_flag_bits: ReservedBitsPolicy) -> Self {
        self.reserved_flag_bits = reserved_flag_bits;
        self
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ChunkIdPlan {
    first_counter: u32,
    count: u32,
    next_counter: u32,
}

impl ChunkIdPlan {
    pub(super) const fn new(next_id: u32, count: u32) -> Result<Self, EditError> {
        let next_counter = match next_id.checked_add(count) {
            Some(counter) => counter,
            None => return Err(EditError::ChunkIdExhausted),
        };
        Ok(Self {
            first_counter: next_id,
            count,
            next_counter,
        })
    }

    pub(super) const fn get(self, offset: u32) -> Option<ChunkId> {
        if offset >= self.count {
            return None;
        }
        match self.first_counter.checked_add(offset) {
            Some(counter) => Some(ChunkId::new(counter)),
            None => None,
        }
    }

    pub(super) const fn next_counter(self) -> u32 {
        self.next_counter
    }
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
    #[cfg(test)]
    pub(super) fn replace_raw(
        &mut self,
        id: ChunkId,
        payload: PayloadInput<'a>,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let index = chunk_index(&self.state, id)?;
        self.replace_raw_at(index, payload, policy)
    }

    /// Appends one encoded chunk without copying its payload bytes.
    ///
    /// A FLAT document is promoted atomically before the append. The returned
    /// identity belongs to the appended node; use [`Document::promote_to_chunk`]
    /// when the promoted IMAGE identity is also needed.
    pub(crate) fn push_raw(&mut self, input: RawChunkInput<'a>) -> Result<ChunkId, EditError> {
        self.insert_raw_at(InsertPosition::End, input)
    }

    /// Inserts one encoded chunk immediately before `anchor`.
    pub(crate) fn insert_raw_before(
        &mut self,
        anchor: ChunkId,
        input: RawChunkInput<'a>,
    ) -> Result<ChunkId, EditError> {
        self.insert_raw_at(InsertPosition::Before(anchor), input)
    }

    /// Inserts one encoded chunk immediately after `anchor`.
    pub(crate) fn insert_raw_after(
        &mut self,
        anchor: ChunkId,
        input: RawChunkInput<'a>,
    ) -> Result<ChunkId, EditError> {
        self.insert_raw_at(InsertPosition::After(anchor), input)
    }

    #[cfg(test)]
    pub(super) fn replace_raw_at(
        &mut self,
        index: usize,
        payload: PayloadInput<'a>,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        let limits = self.payload_limits;
        self.replace_payload_at_with(
            index,
            || Ok(payload),
            |chunk_type, flags, payload| {
                prepare_replacement(chunk_type, flags, payload, policy, limits)
            },
        )
    }

    pub(super) fn replace_extension_at(
        &mut self,
        index: usize,
        input: RawChunkInput<'a>,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let limits = self.payload_limits;
        let prepared = prepare_raw(input, limits)?;
        let (is_primary, unchanged) = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout checked before preparing extension replacement");
            };
            ensure_primary_projection(
                chunks,
                PrimaryProjection::SetType {
                    index,
                    chunk_type: prepared.chunk_type,
                },
            )?;
            let node = &chunks.chunks[index];
            let payload = resolve_node_payload(self, node).map_err(EditError::InvalidPayload)?;
            let candidate = prepared
                .payload
                .resolve_contiguous(&self.origin)
                .expect("prepared extension payload is contiguous");
            (
                chunks.primary == Some(node.id),
                node.chunk_type == prepared.chunk_type
                    && node.flags == prepared.flags
                    && node.capability == prepared.capability
                    && payload.equals(candidate),
            )
        };
        if unchanged {
            return Ok(());
        }
        self.commit_prepared_replacement(index, is_primary, prepared)
    }

    pub(super) fn replace_typed_owned_with<P, Plan, Equal, Encode, ResolveError>(
        &mut self,
        id: ChunkId,
        chunk_type: ChunkType,
        plan: Plan,
        equal: Equal,
        encode: Encode,
        resolve_error: ResolveError,
    ) -> Result<(), EditError>
    where
        Plan: FnOnce() -> Result<P, EditError>,
        for<'payload> Equal: FnOnce(&P, ResolvedNodePayload<'payload>) -> Result<bool, EditError>,
        Encode: FnOnce(P) -> Result<Vec<u8>, EditError>,
        ResolveError: FnOnce(ImagePayloadError) -> EditError,
    {
        self.ensure_mutable()?;
        let index = chunk_index(&self.state, id)?;
        let (actual_type, flags, capability, is_primary) = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout checked before preparing typed replacement");
            };
            let node = &chunks.chunks[index];
            (
                node.chunk_type,
                node.flags,
                node.capability,
                chunks.primary == Some(node.id),
            )
        };
        if actual_type != chunk_type {
            return Err(EditError::InvalidChunkType);
        }

        let plan = plan()?;
        let matches_existing = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout checked before comparing typed replacement");
            };
            let existing =
                resolve_node_payload(self, &chunks.chunks[index]).map_err(resolve_error)?;
            equal(&plan, existing)?
        };
        if matches_existing {
            return Ok(());
        }

        let reserved_policy = if capability.preserves_reserved_bits() {
            ReservedBitsPolicy::Preserve
        } else {
            ReservedBitsPolicy::Reject
        };
        let flags = reserved_policy.apply(flags)?.flags;
        let payload = encode(plan)?;
        self.commit_prepared_replacement(
            index,
            is_primary,
            PreparedRaw {
                chunk_type,
                flags,
                payload: PayloadStorage::Owned(payload),
                capability: RewriteCapability::new(
                    true,
                    true,
                    capability.preserves_reserved_bits(),
                ),
            },
        )
    }

    #[cfg(test)]
    fn replace_payload_at_with<C, P>(
        &mut self,
        index: usize,
        candidate: C,
        prepare: P,
    ) -> Result<(), EditError>
    where
        C: FnOnce() -> Result<PayloadInput<'a>, EditError>,
        P: FnOnce(ChunkType, ChunkFlags, PayloadInput<'a>) -> Result<PreparedRaw<'a>, EditError>,
    {
        self.ensure_mutable()?;
        let (chunk_type, flags, is_primary) = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout checked before preparing replacement");
            };
            let node = &chunks.chunks[index];
            (node.chunk_type, node.flags, chunks.primary == Some(node.id))
        };
        let payload = candidate()?;
        let matches_existing = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout checked before comparing replacement");
            };
            resolve_node_payload(self, &chunks.chunks[index])
                .map_err(EditError::InvalidPayload)?
                .equals(payload.as_bytes())
        };

        if matches_existing {
            return Ok(());
        }

        let prepared = prepare(chunk_type, flags, payload)?;
        self.commit_prepared_replacement(index, is_primary, prepared)
    }

    fn commit_prepared_replacement(
        &mut self,
        index: usize,
        is_primary: bool,
        prepared: PreparedRaw<'a>,
    ) -> Result<(), EditError> {
        let primary_hints = if is_primary {
            let payload = match &prepared.payload {
                PayloadStorage::Borrowed(bytes) => *bytes,
                PayloadStorage::Owned(bytes) => bytes.as_slice(),
                PayloadStorage::SourceRange(_) => {
                    unreachable!("replacement input cannot produce source-backed storage")
                }
                PayloadStorage::PromotedFlat => {
                    unreachable!("replacement input cannot produce promoted storage")
                }
            };
            Some(changed_primary_hint_state(
                prepared.chunk_type,
                ResolvedNodePayload::Contiguous {
                    bytes: payload,
                    placement: PayloadPlacement::Unplaced,
                },
                self.payload_limits,
            ))
        } else {
            None
        };
        let DocumentState::Chunk(chunks) = &mut self.state else {
            unreachable!("layout checked before committing replacement");
        };
        let node = &mut chunks.chunks[index];
        let replaced_promoted = matches!(node.payload, PayloadStorage::PromotedFlat);
        node.chunk_type = prepared.chunk_type;
        node.flags = prepared.flags;
        node.payload = prepared.payload;
        node.capability = prepared.capability;
        if replaced_promoted {
            chunks
                .promoted_flat
                .take()
                .expect("promoted payload tag requires its FLAT sidecar");
        }
        if let Some(primary_hints) = primary_hints {
            chunks.primary_hints = primary_hints;
        }
        self.dirty = true;
        Ok(())
    }

    /// Removes `id` without materializing its payload bytes.
    pub fn remove(&mut self, id: ChunkId) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let index = chunk_index(&self.state, id)?;
        self.remove_at(index);
        Ok(())
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
        R: FnOnce(&mut Vec<ChunkNode<'a>>, usize) -> Result<(), EditError>,
    {
        let chunk_type = input.chunk_type;
        let limits = self.payload_limits;
        self.insert_prepared_at_with(position, chunk_type, || prepare_raw(input, limits), reserve)
    }

    fn insert_prepared_at_with<P, R>(
        &mut self,
        position: InsertPosition,
        chunk_type: ChunkType,
        prepare: P,
        reserve: R,
    ) -> Result<ChunkId, EditError>
    where
        P: FnOnce() -> Result<PreparedRaw<'a>, EditError>,
        R: FnOnce(&mut Vec<ChunkNode<'a>>, usize) -> Result<(), EditError>,
    {
        self.ensure_mutable()?;
        match (&self.state, position) {
            (DocumentState::Flat(_), InsertPosition::End) => {
                return self.insert_prepared_into_flat_with(prepare, reserve);
            }
            (DocumentState::Flat(_), _) => {
                return Err(EditError::ChunkLayoutRequired);
            }
            (DocumentState::Chunk(_), _) => {}
        }
        let index = insertion_index(&self.state, position)?;
        let ids = ChunkIdPlan::new(self.next_id, 1)?;
        let id = ids.get(0).expect("one planned chunk ID must exist");
        let DocumentState::Chunk(chunks) = &self.state else {
            unreachable!("layout checked before projecting insertion");
        };
        ensure_primary_projection(chunks, PrimaryProjection::Insert { index, chunk_type })?;
        let prepared = prepare()?;
        debug_assert_eq!(prepared.chunk_type, chunk_type);
        let node = prepared.into_node(id);

        let DocumentState::Chunk(chunks) = &mut self.state else {
            unreachable!("layout checked before preparing insertion");
        };
        reserve(&mut chunks.chunks, 1)?;
        chunks.chunks.insert(index, node);

        self.next_id = ids.next_counter();
        self.dirty = true;
        Ok(id)
    }

    pub(super) fn push_typed_owned_with<F>(
        &mut self,
        chunk_type: ChunkType,
        flags: ChunkFlags,
        encode: F,
    ) -> Result<ChunkId, EditError>
    where
        F: FnOnce() -> Result<Vec<u8>, EditError>,
    {
        self.insert_prepared_at_with(
            InsertPosition::End,
            chunk_type,
            || prepare_typed_owned_with(chunk_type, flags, encode),
            reserve_one_node,
        )
    }

    #[cfg(test)]
    pub(super) fn insert_raw_at_end_with<R>(
        &mut self,
        input: RawChunkInput<'a>,
        reserve: R,
    ) -> Result<ChunkId, EditError>
    where
        R: FnOnce(&mut Vec<ChunkNode<'a>>, usize) -> Result<(), EditError>,
    {
        self.insert_raw_at_with(InsertPosition::End, input, reserve)
    }

    fn insert_prepared_into_flat_with<P, R>(
        &mut self,
        prepare: P,
        reserve: R,
    ) -> Result<ChunkId, EditError>
    where
        P: FnOnce() -> Result<PreparedRaw<'a>, EditError>,
        R: FnOnce(&mut Vec<ChunkNode<'a>>, usize) -> Result<(), EditError>,
    {
        let ids = ChunkIdPlan::new(self.next_id, 2)?;
        let promoted_id = ids.get(0).expect("first planned chunk ID must exist");
        let inserted_id = ids.get(1).expect("second planned chunk ID must exist");
        let prepared = prepare()?;

        let mut nodes = Vec::new();
        reserve(&mut nodes, 2)?;
        nodes.push(promoted_flat_node(promoted_id));
        nodes.push(prepared.into_node(inserted_id));

        let previous = core::mem::replace(
            &mut self.state,
            DocumentState::Chunk(ChunkSet {
                chunks: Vec::new(),
                primary: None,
                primary_hints: PrimaryHintState::Missing,
                promoted_flat: None,
            }),
        );
        let DocumentState::Flat(record) = previous else {
            unreachable!("FLAT layout was checked before raw insertion")
        };
        self.state = DocumentState::Chunk(promoted_chunk_set(record, nodes, promoted_id));
        self.next_id = ids.next_counter();
        self.dirty = true;
        Ok(inserted_id)
    }

    pub(super) fn remove_to_vec_with<C>(
        &mut self,
        id: ChunkId,
        copy: C,
    ) -> Result<Vec<u8>, EditError>
    where
        C: FnOnce(ResolvedNodePayload<'_>) -> Result<Vec<u8>, EditError>,
    {
        self.ensure_mutable()?;
        let index = chunk_index(&self.state, id)?;
        let copied = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout checked before materializing removal");
            };
            match &chunks.chunks[index].payload {
                PayloadStorage::Owned(_) => None,
                PayloadStorage::Borrowed(_)
                | PayloadStorage::SourceRange(_)
                | PayloadStorage::PromotedFlat => Some(copy(
                    resolve_node_payload(self, &chunks.chunks[index])
                        .map_err(EditError::InvalidPayload)?,
                )?),
            }
        };

        let node = self.remove_at(index);
        match (copied, node.payload) {
            (Some(bytes), PayloadStorage::SourceRange(_) | PayloadStorage::Borrowed(_)) => {
                Ok(bytes)
            }
            (Some(bytes), PayloadStorage::PromotedFlat) => Ok(bytes),
            (None, PayloadStorage::Owned(bytes)) => Ok(bytes),
            _ => unreachable!("payload storage cannot change during removal"),
        }
    }

    fn remove_at(&mut self, index: usize) -> ChunkNode<'a> {
        let DocumentState::Chunk(chunks) = &mut self.state else {
            unreachable!("layout checked before committing removal");
        };
        let node = chunks.chunks.remove(index);
        if matches!(node.payload, PayloadStorage::PromotedFlat) {
            chunks
                .promoted_flat
                .take()
                .expect("promoted payload tag requires its FLAT sidecar");
        }
        let was_primary = chunks.primary == Some(node.id);
        if was_primary {
            chunks.primary = None;
            chunks.primary_hints = super::PrimaryHintState::Missing;
        }
        self.dirty = true;
        node
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

fn prepare_raw(
    input: RawChunkInput<'_>,
    limits: crate::PayloadLimits,
) -> Result<PreparedRaw<'_>, EditError> {
    let descriptor = input.policy.evaluate(
        input.chunk_type,
        input.flags,
        input.payload.as_bytes(),
        limits,
    )?;

    Ok(PreparedRaw {
        chunk_type: descriptor.chunk_type,
        flags: descriptor.flags,
        payload: input.payload.into_storage(),
        capability: descriptor.capability,
    })
}

#[cfg(test)]
fn prepare_replacement(
    chunk_type: ChunkType,
    flags: ChunkFlags,
    payload: PayloadInput<'_>,
    policy: RawChunkPolicy,
    limits: crate::PayloadLimits,
) -> Result<PreparedRaw<'_>, EditError> {
    let evaluated_flags = policy.reserved_flag_bits.apply(flags)?;
    if evaluated_flags.flags != flags {
        return Err(EditError::ReservedFlagBits {
            bits: flags.bits() & !ChunkFlags::CRITICAL.bits(),
        });
    }
    let descriptor =
        policy.evaluate_with_flags(chunk_type, evaluated_flags, payload.as_bytes(), limits)?;

    Ok(PreparedRaw {
        chunk_type: descriptor.chunk_type,
        flags: descriptor.flags,
        payload: payload.into_storage(),
        capability: descriptor.capability,
    })
}

fn prepare_typed_owned_with<'a, F>(
    chunk_type: ChunkType,
    flags: ChunkFlags,
    encode: F,
) -> Result<PreparedRaw<'a>, EditError>
where
    F: FnOnce() -> Result<Vec<u8>, EditError>,
{
    let flags = ReservedBitsPolicy::Reject.apply(flags)?.flags;
    let payload = encode()?;
    Ok(PreparedRaw {
        chunk_type,
        flags,
        payload: PayloadStorage::Owned(payload),
        capability: RewriteCapability::new(true, true, false),
    })
}

fn copy_payload(payload: ResolvedNodePayload<'_>) -> Result<Vec<u8>, EditError> {
    payload.to_vec().map_err(|error| match error {
        EncodeError::AllocationFailed => EditError::AllocationFailed,
        _ => EditError::InvalidPayload(ImagePayloadError::SizeOverflow),
    })
}

fn reserve_one_node(chunks: &mut Vec<ChunkNode<'_>>, additional: usize) -> Result<(), EditError> {
    chunks
        .try_reserve_exact(additional)
        .map_err(|_| EditError::AllocationFailed)
}

#[cfg(test)]
#[path = "../../tests/unit/document/raw/tests.rs"]
mod tests;
