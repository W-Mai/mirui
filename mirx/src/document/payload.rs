use alloc::vec::Vec;

use super::{ChunkNode, Document, DocumentState, FlatRecord, PayloadStorage};
use crate::payload::image::{
    ImageEncodeError, ImageMeta, ImagePayloadError, ImagePayloadPlan, ImagePlanes,
};
use crate::{ChunkType, EncodeError, ImageView, PrimaryHints};

#[derive(Clone, Copy)]
pub(super) enum PayloadPlacement {
    Fixed(u32),
    Unplaced,
}

pub(super) type ResolvedImagePlanes<'a> = ImagePlanes<'a>;

#[derive(Clone, Copy)]
pub(super) enum ResolvedNodePayload<'a> {
    Contiguous {
        bytes: &'a [u8],
        placement: PayloadPlacement,
    },
    PromotedImage(ResolvedImagePlanes<'a>),
}

impl<'a> ResolvedNodePayload<'a> {
    pub(super) const fn bytes(self) -> Option<&'a [u8]> {
        match self {
            Self::Contiguous { bytes, .. } => Some(bytes),
            Self::PromotedImage(_) => None,
        }
    }

    pub(super) fn encoded_len(self) -> Result<usize, EncodeError> {
        match self {
            Self::Contiguous { bytes, .. } => Ok(bytes.len()),
            Self::PromotedImage(image) => Ok(plan_image_payload(image)?.encoded_len()),
        }
    }

    pub(super) fn copy_into(self, out: &mut [u8]) -> Result<usize, EncodeError> {
        match self {
            Self::Contiguous { bytes, .. } => {
                if out.len() < bytes.len() {
                    return Err(EncodeError::BufferTooSmall {
                        needed: bytes.len(),
                        available: out.len(),
                    });
                }
                out[..bytes.len()].copy_from_slice(bytes);
                Ok(bytes.len())
            }
            Self::PromotedImage(image) => plan_image_payload(image)?
                .copy_payload_into(out)
                .map_err(encode_error_for_image_encoding),
        }
    }

    pub(super) fn to_vec(self) -> Result<Vec<u8>, EncodeError> {
        match self {
            Self::Contiguous { bytes, .. } => {
                let mut out = Vec::new();
                out.try_reserve_exact(bytes.len())
                    .map_err(|_| EncodeError::AllocationFailed)?;
                out.extend_from_slice(bytes);
                Ok(out)
            }
            Self::PromotedImage(image) => plan_image_payload(image)?
                .payload_to_vec()
                .map_err(encode_error_for_image_encoding),
        }
    }

    pub(super) fn equals(self, candidate: &[u8]) -> bool {
        match self {
            Self::Contiguous { bytes, .. } => bytes == candidate,
            Self::PromotedImage(image) => {
                plan_image_payload(image).is_ok_and(|plan| plan.equals_payload(candidate))
            }
        }
    }

    pub(super) fn image_hints(self) -> Option<PrimaryHints> {
        self.validate_image_contract().ok()
    }

    pub(super) fn validate_image_contract(self) -> Result<PrimaryHints, ImagePayloadError> {
        self.image_planes().map(image_primary_hints)
    }

    pub(super) fn image_view(self) -> Result<ImageView<'a>, ImagePayloadError> {
        match self {
            Self::Contiguous { bytes, placement } => match placement {
                PayloadPlacement::Fixed(offset) => ImageView::open_payload_at(bytes, offset),
                PayloadPlacement::Unplaced => ImageView::open_payload(bytes),
            },
            Self::PromotedImage(image) => Ok(ImageView::from_validated_planes(
                ImageMeta {
                    width: image.width,
                    height: image.height,
                    stride: image.stride,
                    format: image.format,
                },
                image.main,
                image.extra,
            )),
        }
    }

    pub(super) fn equals_image_plan(self, candidate: ImagePayloadPlan<'_>) -> bool {
        match self {
            Self::Contiguous { bytes, placement } => {
                let aligned = match placement {
                    PayloadPlacement::Fixed(_) => self.image_view().is_ok(),
                    PayloadPlacement::Unplaced => true,
                };
                aligned && candidate.equals_payload(bytes)
            }
            Self::PromotedImage(image) => ImagePayloadPlan::from_planes(image)
                .is_ok_and(|existing| existing.equals_plan(candidate)),
        }
    }

    pub(super) fn image_planes(self) -> Result<ResolvedImagePlanes<'a>, ImagePayloadError> {
        resolved_image_planes(self.image_view()?)
    }
}

fn resolved_image_planes(
    image: ImageView<'_>,
) -> Result<ResolvedImagePlanes<'_>, ImagePayloadError> {
    ImagePlanes::from_view(image)
}

pub(super) fn resolve_flat_record<'document>(
    document: &'document Document<'_>,
    record: &'document FlatRecord<'_>,
) -> Result<ResolvedImagePlanes<'document>, ImagePayloadError> {
    let (main, extra) = record
        .resolve_planes(&document.origin)
        .ok_or(ImagePayloadError::SizeOverflow)?;
    ImagePlanes::new(record.image, main, extra)
}

pub(super) fn resolve_node_payload<'document>(
    document: &'document Document<'_>,
    node: &'document ChunkNode<'_>,
) -> Result<ResolvedNodePayload<'document>, ImagePayloadError> {
    match &node.payload {
        PayloadStorage::SourceRange(range) => Ok(ResolvedNodePayload::Contiguous {
            bytes: document
                .origin
                .resolve(*range)
                .ok_or(ImagePayloadError::SizeOverflow)?,
            placement: PayloadPlacement::Fixed(
                u32::try_from(range.start()).map_err(|_| ImagePayloadError::SizeOverflow)?,
            ),
        }),
        PayloadStorage::Borrowed(bytes) => Ok(ResolvedNodePayload::Contiguous {
            bytes,
            placement: PayloadPlacement::Unplaced,
        }),
        PayloadStorage::Owned(bytes) => Ok(ResolvedNodePayload::Contiguous {
            bytes: bytes.as_slice(),
            placement: PayloadPlacement::Unplaced,
        }),
        PayloadStorage::PromotedFlat => {
            let DocumentState::Chunk(chunks) = &document.state else {
                unreachable!("promoted payload requires CHUNK state");
            };
            let record = chunks
                .promoted_flat
                .as_ref()
                .expect("promoted payload tag requires its FLAT sidecar");
            resolve_flat_record(document, record).map(ResolvedNodePayload::PromotedImage)
        }
    }
}

pub(super) fn encode_error_for_image(_: ImagePayloadError) -> EncodeError {
    EncodeError::InvalidPayload {
        chunk_type: ChunkType::IMAGE,
    }
}

fn image_primary_hints(image: ResolvedImagePlanes<'_>) -> PrimaryHints {
    PrimaryHints::new(
        image.format.to_u8(),
        image.width,
        image.height,
        image.stride,
    )
}

fn plan_image_payload(image: ResolvedImagePlanes<'_>) -> Result<ImagePayloadPlan<'_>, EncodeError> {
    ImagePayloadPlan::from_planes(image).map_err(|error| {
        debug_assert_eq!(error, ImagePayloadError::SizeOverflow);
        EncodeError::SizeOverflow
    })
}

fn encode_error_for_image_encoding(error: ImageEncodeError) -> EncodeError {
    match error {
        ImageEncodeError::InvalidPayload(_) => EncodeError::InvalidPayload {
            chunk_type: ChunkType::IMAGE,
        },
        ImageEncodeError::BufferTooSmall { needed, available } => {
            EncodeError::BufferTooSmall { needed, available }
        }
        ImageEncodeError::AllocationFailed => EncodeError::AllocationFailed,
    }
}
