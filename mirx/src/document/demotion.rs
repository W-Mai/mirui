use super::payload::{ResolvedImagePlanes, ResolvedNodePayload, resolve_node_payload};
use super::{ChunkSet, Document, DocumentState, FileMeta, FlatRecord, PayloadStorage, SourceRange};
use crate::{ChunkFlags, ChunkType, EditError, FLAT_HEADER_LEN};

#[derive(Clone, Copy)]
pub(super) struct FlatCandidate<'a> {
    pub(super) image: ResolvedImagePlanes<'a>,
    storage: CandidateStorage,
}

#[derive(Clone, Copy)]
enum CandidateStorage {
    Promoted,
    Contiguous {
        main: SourceRange,
        extra: Option<SourceRange>,
    },
}

#[derive(Clone, Copy)]
struct DemotionPlan {
    image: crate::payload::image::ImageMeta,
    storage: CandidateStorage,
}

impl Document<'_> {
    /// Converts one losslessly representable CHUNK IMAGE to FLAT layout.
    ///
    /// A current FLAT document is an exact no-op and returns `false`. A CHUNK
    /// document must contain exactly one unflagged, uncompressed IMAGE and that
    /// node must be primary. The IMAGE envelope and plane sizes are validated
    /// before state changes. Existing source-backed, borrowed, and owned
    /// storage remains in place; no image plane is copied.
    ///
    /// A successful conversion invalidates the removed CHUNK identity. The
    /// session counter is retained, so a later promotion assigns a fresh ID.
    pub fn try_demote_to_flat(&mut self) -> Result<bool, EditError> {
        self.ensure_mutable()?;
        match &self.state {
            DocumentState::Flat(_) | DocumentState::OpaqueFlat(_) => return Ok(false),
            DocumentState::Chunk(_) => {}
        }

        let plan = {
            let DocumentState::Chunk(chunks) = &self.state else {
                unreachable!("layout checked before FLAT demotion planning");
            };
            let candidate = flat_candidate(self, chunks)?;
            DemotionPlan {
                image: crate::payload::image::ImageMeta {
                    width: candidate.image.width,
                    height: candidate.image.height,
                    stride: candidate.image.stride,
                    format: candidate.image.format,
                },
                storage: candidate.storage,
            }
        };

        let record = {
            let DocumentState::Chunk(chunks) = &mut self.state else {
                unreachable!("layout checked before FLAT demotion commit");
            };
            let node = chunks
                .chunks
                .pop()
                .expect("validated demotion candidate must contain one node");
            match plan.storage {
                CandidateStorage::Promoted => {
                    debug_assert!(matches!(node.payload, PayloadStorage::PromotedFlat));
                    chunks
                        .promoted_flat
                        .take()
                        .expect("promoted payload tag requires its FLAT sidecar")
                }
                CandidateStorage::Contiguous { main, extra } => {
                    debug_assert!(!matches!(node.payload, PayloadStorage::PromotedFlat));
                    debug_assert!(chunks.promoted_flat.is_none());
                    FlatRecord::from_payload(plan.image, node.payload, main, extra)
                }
            }
        };
        self.state = DocumentState::Flat(record);
        self.dirty = true;
        Ok(true)
    }
}

pub(super) fn flat_candidate<'document>(
    document: &'document Document<'_>,
    chunks: &'document ChunkSet<'_>,
) -> Result<FlatCandidate<'document>, EditError> {
    if document.file != FileMeta::CURRENT || chunks.chunks.len() != 1 {
        return Err(EditError::NotRepresentableAsFlat);
    }
    let node = &chunks.chunks[0];
    if node.chunk_type != ChunkType::IMAGE
        || node.flags != ChunkFlags::NONE
        || chunks.primary != Some(node.id)
        || !node.capability.is_relocatable()
    {
        return Err(EditError::NotRepresentableAsFlat);
    }

    let payload =
        resolve_node_payload(document, node).map_err(|_| EditError::NotRepresentableAsFlat)?;
    let image = payload
        .image_planes()
        .map_err(|_| EditError::NotRepresentableAsFlat)?;
    let file_size = (FLAT_HEADER_LEN as u32)
        .checked_add(image.main_size)
        .and_then(|size| size.checked_add(image.extra_size))
        .ok_or(EditError::NotRepresentableAsFlat)?;
    usize::try_from(file_size).map_err(|_| EditError::NotRepresentableAsFlat)?;
    let storage = match payload {
        ResolvedNodePayload::PromotedImage(_) => CandidateStorage::Promoted,
        ResolvedNodePayload::Contiguous { bytes, .. } => {
            contiguous_plane_ranges(bytes.len(), image)?
        }
    };
    Ok(FlatCandidate { image, storage })
}

fn contiguous_plane_ranges(
    payload_len: usize,
    image: ResolvedImagePlanes<'_>,
) -> Result<CandidateStorage, EditError> {
    let extra_len = image.extra.map_or(0, <[u8]>::len);
    let data_len = image
        .main
        .len()
        .checked_add(extra_len)
        .ok_or(EditError::NotRepresentableAsFlat)?;
    let main_start = payload_len
        .checked_sub(data_len)
        .ok_or(EditError::NotRepresentableAsFlat)?;
    let main = SourceRange::checked(main_start, image.main.len(), payload_len)
        .ok_or(EditError::NotRepresentableAsFlat)?;
    let extra_start = main_start
        .checked_add(image.main.len())
        .ok_or(EditError::NotRepresentableAsFlat)?;
    let extra = if extra_len == 0 {
        None
    } else {
        Some(
            SourceRange::checked(extra_start, extra_len, payload_len)
                .ok_or(EditError::NotRepresentableAsFlat)?,
        )
    };
    Ok(CandidateStorage::Contiguous { main, extra })
}
