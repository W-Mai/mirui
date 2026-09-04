use alloc::vec::Vec;

use crate::media::{MEDIA_HEADER_LEN, MEDIA_SECTION_LEN, MediaPayload, MediaSectionKind};

pub(crate) fn refresh_crc(payload: &mut [u8]) {
    let end = payload.len() - 4;
    let checksum = crate::crc32::compute(&payload[..end]);
    payload[end..].copy_from_slice(&checksum.to_le_bytes());
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
    payload[6] = alignment_log2;
    let size = payload.len() as u32;
    payload[12..16].copy_from_slice(&size.to_le_bytes());
    refresh_crc(&mut payload);
    payload
}
