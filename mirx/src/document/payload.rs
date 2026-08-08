use alloc::vec::Vec;

use super::{ChunkNode, Document, DocumentState, FlatRecord, PayloadStorage};
use crate::payload::image::{ImagePayloadError, validate_image_planes};
use crate::{ChunkType, ColorFormat, EncodeError, ImageChunkHeader, ImageView, PrimaryHints};

#[derive(Clone, Copy)]
pub(super) enum PayloadPlacement {
    Fixed(u32),
    Unplaced,
}

#[derive(Clone, Copy)]
pub(super) struct ResolvedImagePlanes<'a> {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: ColorFormat,
    pub(super) stride: u32,
    pub(super) main: &'a [u8],
    pub(super) extra: Option<&'a [u8]>,
    pub(super) main_size: u32,
    pub(super) extra_size: u32,
}

impl ResolvedImagePlanes<'_> {
    pub(super) const fn primary_hints(self) -> PrimaryHints {
        PrimaryHints::new(self.format.to_u8(), self.width, self.height, self.stride)
    }

    pub(super) fn encoded_len(self) -> Result<usize, EncodeError> {
        let len = (ImageChunkHeader::SIZE as u32)
            .checked_add(self.main_size)
            .and_then(|size| size.checked_add(self.extra_size))
            .ok_or(EncodeError::SizeOverflow)?;
        usize::try_from(len).map_err(|_| EncodeError::SizeOverflow)
    }

    pub(super) fn copy_payload_into(self, out: &mut [u8]) -> Result<usize, EncodeError> {
        let needed = self.encoded_len()?;
        if out.len() < needed {
            return Err(EncodeError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }
        self.emit_payload(&mut out[..needed]);
        Ok(needed)
    }

    pub(super) fn payload_to_vec(self) -> Result<Vec<u8>, EncodeError> {
        let needed = self.encoded_len()?;
        let mut out = Vec::new();
        out.try_reserve_exact(needed)
            .map_err(|_| EncodeError::AllocationFailed)?;
        out.resize(needed, 0);
        self.emit_payload(&mut out);
        Ok(out)
    }

    pub(super) fn equals_payload(self, candidate: &[u8]) -> bool {
        let Ok(expected_len) = self.encoded_len() else {
            return false;
        };
        if candidate.len() != expected_len {
            return false;
        }

        let mut header = [0; ImageChunkHeader::SIZE];
        self.write_header(&mut header);
        let main_end = ImageChunkHeader::SIZE + self.main.len();
        candidate[..ImageChunkHeader::SIZE] == header
            && candidate[ImageChunkHeader::SIZE..main_end] == *self.main
            && match self.extra {
                Some(extra) => candidate[main_end..] == *extra,
                None => candidate.len() == main_end,
            }
    }

    pub(super) fn emit_payload(self, out: &mut [u8]) {
        debug_assert_eq!(self.encoded_len(), Ok(out.len()));
        out.fill(0);
        self.write_header(&mut out[..ImageChunkHeader::SIZE]);

        let data = &mut out[ImageChunkHeader::SIZE..];
        let (main, extra) = data.split_at_mut(self.main.len());
        main.copy_from_slice(self.main);
        match self.extra {
            Some(source) => extra.copy_from_slice(source),
            None => debug_assert!(extra.is_empty()),
        }
    }

    fn write_header(self, out: &mut [u8]) {
        debug_assert_eq!(out.len(), ImageChunkHeader::SIZE);
        write_u32(out, 0, self.width);
        write_u32(out, 4, self.height);
        out[8] = self.format.to_u8();
        write_u32(out, 12, self.stride);
        write_u32(out, 16, ImageChunkHeader::SIZE as u32);
        write_u32(
            out,
            20,
            self.main_size
                .checked_add(self.extra_size)
                .expect("validated IMAGE plane sizes must fit the payload header"),
        );
        write_u32(out, 24, self.extra_size);
    }
}

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
            Self::PromotedImage(image) => image.encoded_len(),
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
            Self::PromotedImage(image) => image.copy_payload_into(out),
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
            Self::PromotedImage(image) => image.payload_to_vec(),
        }
    }

    pub(super) fn equals(self, candidate: &[u8]) -> bool {
        match self {
            Self::Contiguous { bytes, .. } => bytes == candidate,
            Self::PromotedImage(image) => image.equals_payload(candidate),
        }
    }

    pub(super) fn image_hints(self) -> Option<PrimaryHints> {
        self.validate_image_contract().ok()
    }

    pub(super) fn validate_image_contract(self) -> Result<PrimaryHints, ImagePayloadError> {
        self.image_planes().map(ResolvedImagePlanes::primary_hints)
    }

    pub(super) fn image_planes(self) -> Result<ResolvedImagePlanes<'a>, ImagePayloadError> {
        match self {
            Self::Contiguous { bytes, placement } => {
                let image = match placement {
                    PayloadPlacement::Fixed(offset) => ImageView::open_payload_at(bytes, offset)?,
                    PayloadPlacement::Unplaced => ImageView::open_payload(bytes)?,
                };
                resolved_image_planes(image)
            }
            Self::PromotedImage(image) => Ok(image),
        }
    }
}

fn resolved_image_planes(
    image: ImageView<'_>,
) -> Result<ResolvedImagePlanes<'_>, ImagePayloadError> {
    let main_size =
        u32::try_from(image.main().len()).map_err(|_| ImagePayloadError::SizeOverflow)?;
    let extra_size = u32::try_from(image.extra().map_or(0, <[u8]>::len))
        .map_err(|_| ImagePayloadError::SizeOverflow)?;
    Ok(ResolvedImagePlanes {
        width: image.width(),
        height: image.height(),
        format: image.format(),
        stride: image.stride(),
        main: image.main(),
        extra: image.extra(),
        main_size,
        extra_size,
    })
}

pub(super) fn resolve_flat_record<'document>(
    document: &'document Document<'_>,
    record: &'document FlatRecord<'_>,
) -> Result<ResolvedImagePlanes<'document>, ImagePayloadError> {
    let (main, extra) = record
        .resolve_planes(&document.origin)
        .ok_or(ImagePayloadError::SizeOverflow)?;
    let sizes = validate_image_planes(record.image, main, extra)?;

    Ok(ResolvedImagePlanes {
        width: record.image.width,
        height: record.image.height,
        format: record.image.format,
        stride: record.image.stride,
        main,
        extra,
        main_size: sizes.main,
        extra_size: sizes.extra,
    })
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

fn write_u32(out: &mut [u8], offset: usize, value: u32) {
    out[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
