use alloc::vec;
use alloc::vec::Vec;

use super::*;
use crate::header::{CHUNK_TABLE_ENTRY_LEN, chunk_type};
use crate::{crc32, encode_chunks};

fn set_file_size(bytes: &mut [u8], file_size: u32) {
    bytes[16..20].copy_from_slice(&file_size.to_le_bytes());
    refresh_crc(bytes);
}

fn set_table_offset(bytes: &mut [u8], table_offset: u32) {
    bytes[12..16].copy_from_slice(&table_offset.to_le_bytes());
    refresh_crc(bytes);
}

fn set_payload_range(bytes: &mut [u8], index: usize, offset: u32, size: u32) {
    let entry = CHUNK_FILE_HEADER_LEN + index * CHUNK_TABLE_ENTRY_LEN;
    bytes[entry + 4..entry + 8].copy_from_slice(&offset.to_le_bytes());
    bytes[entry + 8..entry + 12].copy_from_slice(&size.to_le_bytes());
}

fn refresh_crc(bytes: &mut [u8]) {
    let checksum = crc32(&bytes[..40]);
    bytes[40..44].copy_from_slice(&checksum.to_le_bytes());
}

fn preserve() -> ReadOptions {
    ReadOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve)
}

#[test]
fn table_must_fit_inside_the_declared_file() {
    let mut bytes = encode_chunks(&[(chunk_type::META, 0, b"")]);
    bytes.extend_from_slice(&[0; CHUNK_TABLE_ENTRY_LEN]);
    set_file_size(&mut bytes, CHUNK_FILE_HEADER_LEN as u32);

    let expected = Err(ReadError::ChunkTableOutOfBounds {
        offset: CHUNK_FILE_HEADER_LEN as u32,
        count: 1,
        file_size: CHUNK_FILE_HEADER_LEN as u32,
    });
    assert_eq!(Reader::open(&bytes), expected);
    assert_eq!(Reader::open_with(&bytes, &preserve()), expected);

    let mut empty = encode_chunks(&[]);
    empty.push(0);
    set_table_offset(&mut empty, CHUNK_FILE_HEADER_LEN as u32 + 1);
    assert!(matches!(
        Reader::open_with(&empty, &preserve()),
        Err(ReadError::ChunkTableOutOfBounds { count: 0, .. })
    ));
}

#[test]
fn table_and_empty_payload_may_end_at_logical_eof() {
    let bytes = encode_chunks(&[(chunk_type::META, 0, b"")]);
    let reader = Reader::open(&bytes).unwrap();
    let chunk = reader.chunks().next().unwrap();
    assert_eq!(chunk.payload(), b"");
    assert_eq!(chunk.payload_offset() as usize, reader.logical_len());

    let mut empty = encode_chunks(&[]);
    let empty_len = empty.len() as u32;
    set_table_offset(&mut empty, empty_len);
    assert_eq!(Reader::open(&empty).unwrap().chunks().len(), 0);
}

#[test]
fn payload_cannot_borrow_preserved_trailing_bytes() {
    for (start_delta, size) in [(0, 4), (-2, 4), (1, 0)] {
        let mut bytes = encode_chunks(&[(chunk_type::META, 0, b"body")]);
        let logical_len = bytes.len();
        bytes.extend_from_slice(b"tail");
        let offset = (logical_len as i64 + start_delta) as u32;
        set_payload_range(&mut bytes, 0, offset, size);

        assert_eq!(
            Reader::open_with(&bytes, &preserve()),
            Err(ReadError::ChunkPayloadOutOfBounds {
                index: 0,
                offset,
                size,
            })
        );
    }
}

#[test]
fn payload_ranges_cannot_point_into_the_header() {
    for (offset, size) in [(0, 1), (43, 1), (43, 2), (43, 0)] {
        let mut bytes = encode_chunks(&[(chunk_type::META, 0, b"body")]);
        set_payload_range(&mut bytes, 0, offset, size);
        assert_eq!(
            Reader::open(&bytes),
            Err(ReadError::ChunkPayloadOverlapsHeader {
                index: 0,
                offset,
                size,
            })
        );
    }
}

#[test]
fn payload_ranges_cannot_point_into_the_table() {
    let table_start = CHUNK_FILE_HEADER_LEN as u32;
    let table_end = table_start + CHUNK_TABLE_ENTRY_LEN as u32;
    for (offset, size) in [
        (table_start, 1),
        (table_start + 1, CHUNK_TABLE_ENTRY_LEN as u32),
        (table_start, 0),
    ] {
        let mut bytes = encode_chunks(&[(chunk_type::META, 0, b"body")]);
        set_payload_range(&mut bytes, 0, offset, size);
        assert_eq!(
            Reader::open(&bytes),
            Err(ReadError::ChunkPayloadOverlapsTable {
                index: 0,
                offset,
                size,
            })
        );
    }

    let mut boundary = encode_chunks(&[(chunk_type::META, 0, b"body")]);
    set_payload_range(&mut boundary, 0, table_end, 4);
    assert_eq!(
        Reader::open(&boundary)
            .unwrap()
            .chunks()
            .next()
            .unwrap()
            .payload(),
        b"body"
    );
}

#[test]
fn payload_may_use_the_gap_before_a_relocated_table() {
    let base = encode_chunks(&[(chunk_type::META, 0, b"body")]);
    let table_offset = 64usize;
    let table_end = table_offset + CHUNK_TABLE_ENTRY_LEN;
    let mut bytes = vec![0; table_end];
    bytes[..CHUNK_FILE_HEADER_LEN].copy_from_slice(&base[..CHUNK_FILE_HEADER_LEN]);
    bytes[44..48].copy_from_slice(b"body");
    bytes[table_offset..table_end].copy_from_slice(
        &base[CHUNK_FILE_HEADER_LEN..CHUNK_FILE_HEADER_LEN + CHUNK_TABLE_ENTRY_LEN],
    );
    bytes[table_offset + 4..table_offset + 8].copy_from_slice(&44u32.to_le_bytes());
    set_table_offset(&mut bytes, table_offset as u32);
    set_file_size(&mut bytes, table_end as u32);

    assert_eq!(
        Reader::open(&bytes)
            .unwrap()
            .chunks()
            .next()
            .unwrap()
            .payload(),
        b"body"
    );

    bytes[table_offset + 4..table_offset + 8].copy_from_slice(&60u32.to_le_bytes());
    bytes[table_offset + 8..table_offset + 12].copy_from_slice(&8u32.to_le_bytes());
    assert_eq!(
        Reader::open(&bytes),
        Err(ReadError::ChunkPayloadOverlapsTable {
            index: 0,
            offset: 60,
            size: 8,
        })
    );
}

#[test]
fn overlapping_payloads_remain_legal() {
    let mut bytes = encode_chunks(&[
        (chunk_type::META, 0, b"abcdef"),
        (chunk_type::FONT, 0, b"uvwxyz"),
    ]);
    let first_offset = u32::from_le_bytes(
        bytes[CHUNK_FILE_HEADER_LEN + 4..CHUNK_FILE_HEADER_LEN + 8]
            .try_into()
            .unwrap(),
    );
    set_payload_range(&mut bytes, 1, first_offset + 2, 4);

    let chunks = Reader::open(&bytes)
        .unwrap()
        .chunks()
        .map(|chunk| chunk.payload())
        .collect::<Vec<_>>();
    assert_eq!(chunks, [b"abcdef".as_slice(), b"cdef".as_slice()]);
}
