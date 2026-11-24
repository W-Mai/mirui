use super::payload::{PayloadPlacement, ResolvedNodePayload, resolve_node_payload};
#[cfg(test)]
use super::primary::{PrimaryProjection, changed_primary_hint_state, ensure_primary_projection};
use super::raw::{CriticalAssumption, RawChunkPolicy, RelocationAssumption, ReservedBitsPolicy};
use super::{ChunkNode, Document, DocumentState, EditError, RewriteCapability};
use crate::frames::FramesView;
use crate::meta::{MetaEncodeError, MetaView};
use crate::palette::{PaletteEncodeError, PaletteView};
use crate::scene::{Scene, VectorEncodeError};
use crate::{ChunkFlags, ChunkType, PayloadLimits};

#[cfg(test)]
use super::PayloadStorage;
#[cfg(test)]
use crate::ChunkId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EvaluatedDescriptor {
    pub(super) chunk_type: ChunkType,
    pub(super) flags: ChunkFlags,
    pub(super) capability: RewriteCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EvaluatedFlags {
    pub(super) flags: ChunkFlags,
    pub(super) preserve_reserved_bits: bool,
}

impl RawChunkPolicy {
    /// Evaluates a complete descriptor candidate without changing document state.
    pub(super) fn evaluate(
        self,
        chunk_type: ChunkType,
        flags: ChunkFlags,
        payload: &[u8],
        limits: PayloadLimits,
    ) -> Result<EvaluatedDescriptor, EditError> {
        let flags = self.reserved_flag_bits.apply(flags)?;
        self.evaluate_resolved(
            chunk_type,
            flags,
            ResolvedNodePayload::Contiguous {
                bytes: payload,
                placement: PayloadPlacement::Unplaced,
            },
            limits,
        )
    }

    #[cfg(test)]
    pub(super) fn evaluate_with_flags(
        self,
        chunk_type: ChunkType,
        evaluated_flags: EvaluatedFlags,
        payload: &[u8],
        limits: PayloadLimits,
    ) -> Result<EvaluatedDescriptor, EditError> {
        self.evaluate_resolved(
            chunk_type,
            evaluated_flags,
            ResolvedNodePayload::Contiguous {
                bytes: payload,
                placement: PayloadPlacement::Unplaced,
            },
            limits,
        )
    }

    fn evaluate_resolved(
        self,
        chunk_type: ChunkType,
        evaluated_flags: EvaluatedFlags,
        payload: ResolvedNodePayload<'_>,
        limits: PayloadLimits,
    ) -> Result<EvaluatedDescriptor, EditError> {
        let flags = evaluated_flags.flags;
        let known_contract = if chunk_type == ChunkType::IMAGE {
            match payload.validate_image_contract(&limits) {
                Ok(_) => true,
                Err(_) if matches!(self.relocation, RelocationAssumption::AssumeRelocatable) => {
                    false
                }
                Err(error) => return Err(EditError::InvalidPayload(error)),
            }
        } else if chunk_type == ChunkType::FONT {
            match payload.bytes() {
                Some(_) => match payload
                    .font_view(&limits)
                    .and_then(|font| font.preflight(&limits))
                {
                    Ok(()) => true,
                    Err(_)
                        if matches!(self.relocation, RelocationAssumption::AssumeRelocatable) =>
                    {
                        false
                    }
                    Err(error) => {
                        return Err(EditError::InvalidFont(error));
                    }
                },
                None if matches!(self.relocation, RelocationAssumption::AssumeRelocatable) => false,
                None => {
                    return Err(EditError::NonContiguousPayload {
                        chunk_type: ChunkType::FONT,
                    });
                }
            }
        } else if chunk_type == ChunkType::VECTOR {
            match payload.bytes() {
                Some(bytes) => match Scene::preflight(bytes, &limits) {
                    Ok(()) => true,
                    Err(_)
                        if matches!(self.relocation, RelocationAssumption::AssumeRelocatable) =>
                    {
                        false
                    }
                    Err(error) => {
                        return Err(EditError::InvalidVector(VectorEncodeError::InvalidPayload(
                            error,
                        )));
                    }
                },
                None if matches!(self.relocation, RelocationAssumption::AssumeRelocatable) => false,
                None => {
                    return Err(EditError::NonContiguousPayload {
                        chunk_type: ChunkType::VECTOR,
                    });
                }
            }
        } else if chunk_type == ChunkType::META {
            match payload.bytes() {
                Some(bytes) => match MetaView::open_payload(bytes, &limits) {
                    Ok(_) => true,
                    Err(_)
                        if matches!(self.relocation, RelocationAssumption::AssumeRelocatable) =>
                    {
                        false
                    }
                    Err(error) => {
                        return Err(EditError::InvalidMeta(MetaEncodeError::InvalidPayload(
                            error,
                        )));
                    }
                },
                None if matches!(self.relocation, RelocationAssumption::AssumeRelocatable) => false,
                None => {
                    return Err(EditError::NonContiguousPayload {
                        chunk_type: ChunkType::META,
                    });
                }
            }
        } else if chunk_type == ChunkType::PALETTE {
            match payload.bytes() {
                Some(bytes) => match PaletteView::open_payload(bytes, &limits) {
                    Ok(_) => true,
                    Err(_)
                        if matches!(self.relocation, RelocationAssumption::AssumeRelocatable) =>
                    {
                        false
                    }
                    Err(error) => {
                        return Err(EditError::InvalidPalette(
                            PaletteEncodeError::InvalidPayload(error),
                        ));
                    }
                },
                None if matches!(self.relocation, RelocationAssumption::AssumeRelocatable) => false,
                None => {
                    return Err(EditError::NonContiguousPayload {
                        chunk_type: ChunkType::PALETTE,
                    });
                }
            }
        } else if chunk_type == ChunkType::FRAMES {
            match payload.bytes() {
                Some(bytes) => match FramesView::open(bytes, &limits)
                    .and_then(FramesView::validate_data)
                {
                    Ok(()) => true,
                    Err(_)
                        if matches!(self.relocation, RelocationAssumption::AssumeRelocatable) =>
                    {
                        false
                    }
                    Err(error) => {
                        return Err(EditError::InvalidFrames(error));
                    }
                },
                None if matches!(self.relocation, RelocationAssumption::AssumeRelocatable) => false,
                None => {
                    return Err(EditError::NonContiguousPayload {
                        chunk_type: ChunkType::FRAMES,
                    });
                }
            }
        } else {
            false
        };

        let relocatable =
            known_contract || matches!(self.relocation, RelocationAssumption::AssumeRelocatable);
        if !relocatable {
            return Err(EditError::RelocationAssumptionRequired { chunk_type });
        }

        let critical_understood = known_contract
            || matches!(
                self.critical_semantics,
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
}

impl EvaluatedDescriptor {
    pub(super) fn for_open(
        chunk_type: ChunkType,
        flags: ChunkFlags,
        known_contract: bool,
        policy: Option<RawChunkPolicy>,
        allow_normalize: bool,
    ) -> (Self, bool) {
        let reserved_bits = flags.bits() & !ChunkFlags::CRITICAL.bits();
        let evaluated_flags = match policy.map(|policy| policy.reserved_flag_bits) {
            Some(ReservedBitsPolicy::Preserve) if reserved_bits != 0 => {
                ReservedBitsPolicy::Preserve
                    .apply(flags)
                    .expect("preserving reserved bits cannot fail")
            }
            Some(ReservedBitsPolicy::Normalize) if reserved_bits != 0 && allow_normalize => {
                ReservedBitsPolicy::Normalize
                    .apply(flags)
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
            Self {
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
}

impl Document<'_> {
    #[cfg(test)]
    pub(super) fn set_type(
        &mut self,
        id: ChunkId,
        chunk_type: ChunkType,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let index = descriptor_chunk_index(&self.state, id)?;
        self.set_type_at(index, chunk_type, policy)
    }

    #[cfg(test)]
    pub(super) fn set_flags(
        &mut self,
        id: ChunkId,
        flags: ChunkFlags,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let index = descriptor_chunk_index(&self.state, id)?;
        self.set_flags_at(index, flags, policy)
    }

    #[cfg(test)]
    pub(super) fn set_raw_policy(
        &mut self,
        id: ChunkId,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let index = descriptor_chunk_index(&self.state, id)?;
        self.set_raw_policy_at(index, policy)
    }

    #[cfg(test)]
    pub(super) fn set_type_at(
        &mut self,
        index: usize,
        chunk_type: ChunkType,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
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
            let flags = policy.reserved_flag_bits.apply(node.flags)?;
            policy.evaluate_resolved(chunk_type, flags, payload, self.payload_limits)?
        };
        self.apply_type_descriptor(index, candidate);
        Ok(())
    }

    pub(super) fn set_flags_at(
        &mut self,
        index: usize,
        flags: ChunkFlags,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let existing_flags = chunk_node(&self.state, index).flags;
        if existing_flags == flags
            && !matches!(policy.reserved_flag_bits, ReservedBitsPolicy::Normalize)
        {
            return Ok(());
        }

        let evaluated_flags = policy.reserved_flag_bits.apply(flags)?;
        if evaluated_flags.flags == existing_flags {
            return Ok(());
        }
        let candidate = {
            let node = chunk_node(&self.state, index);
            let payload = descriptor_payload(self, node)?;
            policy.evaluate_resolved(
                node.chunk_type,
                evaluated_flags,
                payload,
                self.payload_limits,
            )?
        };
        self.apply_flags_descriptor(index, candidate);
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn set_raw_policy_at(
        &mut self,
        index: usize,
        policy: RawChunkPolicy,
    ) -> Result<(), EditError> {
        self.ensure_mutable()?;
        let candidate = {
            let node = chunk_node(&self.state, index);
            let payload = descriptor_payload(self, node)?;
            let flags = policy.reserved_flag_bits.apply(node.flags)?;
            policy.evaluate_resolved(node.chunk_type, flags, payload, self.payload_limits)?
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

    #[cfg(test)]
    fn apply_type_descriptor(&mut self, index: usize, candidate: EvaluatedDescriptor) {
        let primary_hints = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout was checked before planning descriptor edit");
            };
            let node = &chunks.chunks[index];
            if chunks.primary == Some(node.id) {
                let payload = descriptor_payload(self, node)
                    .expect("live document payload must remain resolvable");
                Some(changed_primary_hint_state(
                    candidate.chunk_type,
                    payload,
                    self.payload_limits,
                ))
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

#[cfg(test)]
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
#[path = "../../tests/unit/document/descriptor/tests.rs"]
mod tests;
