use alloc::vec::Vec;

use crate::media::{MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MediaPayload, MediaSectionKind};

pub(crate) fn refresh_crc(payload: &mut [u8]) {
    crate::media::refresh_checksums(payload);
}

pub(crate) fn data_offset(payload: &[u8]) -> usize {
    MediaPayload::open(payload)
        .unwrap()
        .section(MediaSectionKind::DATA)
        .unwrap()
        .descriptor()
        .offset() as usize
}

/// Moves DATA without changing the logical image, and declares file alignment.
pub(crate) fn pad_data(mut payload: Vec<u8>, padding: usize, alignment_log2: u8) -> Vec<u8> {
    if alignment_log2 != 0 {
        let image = super::RawImageView::open(&payload).unwrap();
        let planes: Vec<_> = image.planes().map(|plane| plane.bytes()).collect();
        let layouts: Vec<_> = image
            .planes()
            .map(|plane| {
                let memory = plane.memory();
                super::PlaneMemoryLayout::builder(plane.geometry())
                    .with_allocation_extent(memory.allocation_width(), memory.allocation_height())
                    .with_stride(memory.stride())
                    .with_data_offset(memory.data_offset())
                    .with_alignment(1 << alignment_log2)
                    .with_flags(memory.flags())
                    .build()
                    .unwrap()
            })
            .collect();
        let mut asset =
            super::RawImageAsset::new(image.surface(), &planes).with_memory_layouts(&layouts);
        if let Some(table) = image.color_table() {
            asset = asset.with_color_table(table.as_bytes());
        }
        payload = asset.encode().unwrap();
    }
    let offset = data_offset(&payload);
    let count = MediaPayload::open(&payload)
        .unwrap()
        .header()
        .section_count();
    payload.splice(offset..offset, core::iter::repeat_n(0, padding));
    for index in 0..usize::from(count) {
        let field = MEDIA_HEADER_LEN + index * MEDIA_SECTION_LEN + 4;
        let start = u32::from_le_bytes(payload[field..field + 4].try_into().unwrap());
        if start as usize >= offset {
            payload[field..field + 4].copy_from_slice(&(start + padding as u32).to_le_bytes());
        }
    }
    refresh_crc(&mut payload);
    payload
}
