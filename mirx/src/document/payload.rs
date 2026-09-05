use alloc::vec::Vec;

use super::{ChunkNode, Document, DocumentState, FlatRecord, PayloadStorage};
use crate::image::{EncodedImageAsset, EncodedImageView, ImageRef, RawImageView, SurfaceView};
use crate::payload::image::{ImageEncodeError, ImagePayloadError, ImagePayloadPlan, ImagePlanes};
use crate::{ChunkType, EncodeError, ImageView, PayloadLimits, PrimaryHints};

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
    pub(super) fn frames_view(
        self,
        limits: &PayloadLimits,
    ) -> Result<crate::FramesView<'a>, crate::FramesError> {
        match self {
            Self::Contiguous { bytes, placement } => match placement {
                PayloadPlacement::Fixed(offset) => {
                    crate::FramesView::open_at(bytes, offset, limits)
                }
                PayloadPlacement::Unplaced => crate::FramesView::open(bytes, limits),
            },
            Self::PromotedImage(_) => Err(crate::FramesError::SizeOverflow),
        }
    }

    pub(super) fn font_view(
        self,
        limits: &PayloadLimits,
    ) -> Result<crate::FontView<'a>, crate::FontError> {
        match self {
            Self::Contiguous { bytes, placement } => match placement {
                PayloadPlacement::Fixed(offset) => crate::FontView::open_at(bytes, offset, limits),
                PayloadPlacement::Unplaced => crate::FontView::open(bytes, limits),
            },
            Self::PromotedImage(_) => Err(crate::FontError::SizeOverflow),
        }
    }
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

    pub(super) fn validate_image_contract(
        self,
        limits: &PayloadLimits,
    ) -> Result<PrimaryHints, ImagePayloadError> {
        let view = self.image_view()?;
        let stride = match view {
            ImageRef::Raw(raw) => raw
                .plane(0)
                .expect("validated main plane")
                .memory()
                .stride(),
            ImageRef::Encoded(encoded) => {
                encoded
                    .preflight(limits)
                    .map_err(ImagePayloadError::Encoded)?;
                0
            }
        };
        let surface = view.surface();
        Ok(PrimaryHints::new(
            surface.sample_layout(),
            surface.width(),
            surface.height(),
            stride,
        ))
    }

    pub(super) fn image_view(self) -> Result<ImageRef<'a>, ImagePayloadError> {
        match self {
            Self::Contiguous { bytes, placement } => match placement {
                PayloadPlacement::Fixed(offset) => ImageRef::open_at(bytes, offset),
                PayloadPlacement::Unplaced => ImageRef::open(bytes),
            }
            .map_err(Into::into),
            Self::PromotedImage(image) => {
                ImagePayloadPlan::from_planes(image).map(|plan| ImageRef::Raw(plan.surface()))
            }
        }
    }

    pub(super) fn equals_surface(self, candidate: SurfaceView<'_>) -> bool {
        match self {
            Self::Contiguous { bytes, placement } => {
                let aligned = match placement {
                    PayloadPlacement::Fixed(_) => self.image_view().is_ok(),
                    PayloadPlacement::Unplaced => true,
                };
                aligned && candidate.matches_payload(bytes).unwrap_or(false)
            }
            Self::PromotedImage(_) => self
                .image_view()
                .is_ok_and(|existing| existing.raw() == Some(candidate)),
        }
    }

    pub(super) fn equals_encoded(
        self,
        candidate: EncodedImageAsset<'_>,
    ) -> Result<bool, crate::image::ImageEncodeError> {
        let Self::Contiguous { bytes, placement } = self else {
            return Ok(false);
        };
        if !candidate.matches_payload(bytes)? {
            return Ok(false);
        }
        Ok(match placement {
            PayloadPlacement::Unplaced => true,
            PayloadPlacement::Fixed(offset) => EncodedImageView::open(bytes).is_ok_and(|image| {
                image.input_alignment().is_ok_and(|alignment| {
                    image
                        .media()
                        .validate_file_alignment(offset, alignment)
                        .is_ok()
                })
            }),
        })
    }

    pub(super) fn image_planes(self) -> Result<ResolvedImagePlanes<'a>, ImagePayloadError> {
        let packed = match self {
            Self::Contiguous { bytes, placement } => {
                let raw = match placement {
                    PayloadPlacement::Fixed(offset) => RawImageView::open_at(bytes, offset),
                    PayloadPlacement::Unplaced => RawImageView::open(bytes),
                }
                .map_err(|error| match error {
                    crate::image::RawImageViewError::UnexpectedSection(
                        crate::media::MediaSectionKind::CODINGS,
                    ) => ImagePayloadError::NotRepresentableAsPacked,
                    error => ImagePayloadError::Media(error),
                })?;
                raw.packed()
            }
            Self::PromotedImage(_) => self.image_view()?.raw().and_then(|raw| raw.packed()),
        }
        .ok_or(ImagePayloadError::NotRepresentableAsPacked)?;
        resolved_image_planes(packed)
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

fn plan_image_payload(image: ResolvedImagePlanes<'_>) -> Result<ImagePayloadPlan<'_>, EncodeError> {
    ImagePayloadPlan::from_planes(image).map_err(|error| match error {
        ImagePayloadError::SizeOverflow
        | ImagePayloadError::Surface(crate::image::ImageEncodeError::SizeOverflow) => {
            EncodeError::SizeOverflow
        }
        _ => EncodeError::InvalidPayload {
            chunk_type: ChunkType::IMAGE,
        },
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
