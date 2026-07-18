use alloc::vec;
use alloc::vec::Vec;

use super::*;
use crate::header::{CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, chunk_type};
use crate::{ColorFormat, crc32, encode_chunks, parse_chunk, parse_flat};

fn flat_file(format: ColorFormat, width: u32, height: u32, stride: u32) -> Vec<u8> {
    let main_len = usize::try_from(stride.checked_mul(height).unwrap()).unwrap();
    let extra_len = usize::try_from(format.extra_size(width, height, stride).unwrap()).unwrap();
    let mut bytes = vec![0; FLAT_HEADER_LEN + main_len + extra_len];
    bytes[..4].copy_from_slice(&MAGIC);
    bytes[4] = VERSION_MAJOR;
    bytes[5] = VERSION_MINOR;
    bytes[6] = Layout::Flat.to_u8();
    bytes[8] = format.to_u8();
    bytes[12..16].copy_from_slice(&width.to_le_bytes());
    bytes[16..20].copy_from_slice(&height.to_le_bytes());
    bytes[20..24].copy_from_slice(&stride.to_le_bytes());
    let checksum = crc32(&bytes[..24]);
    bytes[24..28].copy_from_slice(&checksum.to_le_bytes());
    bytes
}

fn set_chunk_file_size(bytes: &mut [u8], file_size: u32) {
    bytes[16..20].copy_from_slice(&file_size.to_le_bytes());
    let checksum = crc32(&bytes[..40]);
    bytes[40..44].copy_from_slice(&checksum.to_le_bytes());
}

#[test]
fn exact_flat_length_includes_main_and_extra_planes() {
    for (format, width, height, stride) in [
        (ColorFormat::RGB565, 3, 2, 8),
        (ColorFormat::I4, 3, 2, 2),
        (ColorFormat::RGB565A8, 3, 2, 8),
    ] {
        let mut bytes = flat_file(format, width, height, stride);
        let logical_len = bytes.len();
        bytes.extend_from_slice(&[0xaa, 0xbb]);

        assert_eq!(
            Reader::open(&bytes),
            Err(ReadError::TrailingBytes {
                logical_len,
                actual_len: logical_len + 2,
            })
        );

        let options = ReadOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
        let reader = Reader::open_with(&bytes, &options).unwrap();
        assert_eq!(reader.logical_len(), logical_len);
        assert_eq!(reader.logical_source(), &bytes[..logical_len]);
        assert_eq!(reader.trailing_bytes(), &[0xaa, 0xbb]);
        assert!(reader.has_trailing_bytes());
        assert_eq!(reader.source(), bytes.as_slice());
        assert_eq!(reader.flat_image().unwrap().format(), format);

        assert!(parse_flat(&bytes).is_ok());
    }
}

#[test]
fn future_flat_treats_the_complete_source_as_opaque() {
    for (minor, flags) in [(VERSION_MINOR + 1, 0), (VERSION_MINOR, 0x80)] {
        let mut bytes = flat_file(ColorFormat::A8, 1, 1, 1);
        bytes[5] = minor;
        bytes[7] = flags;
        bytes[8] = 0xfe;
        bytes[9] = 0x55;
        bytes.extend_from_slice(&[1, 2, 3]);
        let checksum = crc32(&bytes[..24]);
        bytes[24..28].copy_from_slice(&checksum.to_le_bytes());

        let reader = Reader::open(&bytes).unwrap();
        assert!(reader.has_future_semantics());
        assert_eq!(reader.logical_len(), bytes.len());
        assert_eq!(reader.logical_source(), bytes.as_slice());
        assert_eq!(reader.trailing_bytes(), b"");
        assert!(!reader.has_trailing_bytes());
        assert_eq!(reader.flat_image(), None);
    }
}

#[test]
fn chunk_file_size_defines_the_logical_boundary() {
    let mut bytes = encode_chunks(&[(chunk_type::META, 0, b"meta")]);
    let logical_len = bytes.len();
    bytes.extend_from_slice(b"tail");

    assert_eq!(
        Reader::open(&bytes),
        Err(ReadError::TrailingBytes {
            logical_len,
            actual_len: logical_len + 4,
        })
    );

    let options = ReadOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
    let reader = Reader::open_with(&bytes, &options).unwrap();
    assert_eq!(reader.logical_source(), &bytes[..logical_len]);
    assert_eq!(reader.trailing_bytes(), b"tail");
    assert_eq!(reader.chunks().next().unwrap().payload(), b"meta");
    assert!(parse_chunk(&bytes).is_ok());
}

#[test]
fn rejects_invalid_or_truncated_chunk_file_sizes_after_crc_validation() {
    let valid = encode_chunks(&[]);

    let mut too_small = valid.clone();
    set_chunk_file_size(&mut too_small, CHUNK_FILE_HEADER_LEN as u32 - 1);
    assert_eq!(
        Reader::open(&too_small),
        Err(ReadError::InvalidFileSize {
            declared: CHUNK_FILE_HEADER_LEN as u32 - 1,
            minimum: CHUNK_FILE_HEADER_LEN as u32,
        })
    );

    let mut truncated = valid;
    let needed = truncated.len() + 1;
    set_chunk_file_size(&mut truncated, needed as u32);
    assert_eq!(
        Reader::open(&truncated),
        Err(ReadError::Truncated {
            needed,
            available: needed - 1,
        })
    );
}

#[test]
fn future_chunk_headers_keep_the_same_explicit_boundary() {
    for (minor, flags) in [(VERSION_MINOR + 1, 0), (VERSION_MINOR, 0x80)] {
        let mut bytes = encode_chunks(&[(chunk_type::META, 0, b"meta")]);
        let logical_len = bytes.len();
        bytes[5] = minor;
        bytes[7] = flags;
        bytes.extend_from_slice(b"tail");
        let checksum = crc32(&bytes[..40]);
        bytes[40..44].copy_from_slice(&checksum.to_le_bytes());

        assert!(matches!(
            Reader::open(&bytes),
            Err(ReadError::TrailingBytes { .. })
        ));
        let options = ReadOptions::new().with_trailing_bytes(TrailingBytesPolicy::Preserve);
        let reader = Reader::open_with(&bytes, &options).unwrap();
        assert!(reader.has_future_semantics());
        assert_eq!(reader.logical_len(), logical_len);
        assert_eq!(reader.trailing_bytes(), b"tail");
    }
}

#[test]
fn structural_errors_take_precedence_over_trailing_bytes() {
    let mut bytes = encode_chunks(&[(chunk_type::META, 0, b"meta")]);
    bytes.extend_from_slice(b"tail");
    let offset = bytes.len() as u32 + 1;
    bytes[CHUNK_FILE_HEADER_LEN + 4..CHUNK_FILE_HEADER_LEN + 8]
        .copy_from_slice(&offset.to_le_bytes());

    assert_eq!(
        Reader::open(&bytes),
        Err(ReadError::ChunkPayloadOutOfBounds {
            index: 0,
            offset,
            size: 4,
        })
    );
}

#[test]
fn read_options_reject_trailing_bytes_by_default() {
    let options = ReadOptions::default();
    assert_eq!(options.trailing_bytes_policy(), TrailingBytesPolicy::Reject);
    assert_eq!(options.max_chunks(), ReadOptions::DEFAULT_MAX_CHUNKS);
}

#[test]
fn exact_chunk_payload_can_end_at_the_logical_eof() {
    let bytes = encode_chunks(&[(chunk_type::META, 0, b"payload")]);
    let reader = Reader::open(&bytes).unwrap();
    let chunk = reader.chunks().next().unwrap();
    assert_eq!(chunk.payload(), b"payload");
    assert_eq!(
        chunk.payload_offset() as usize + chunk.payload().len(),
        reader.logical_len()
    );
    assert_eq!(
        chunk.payload_offset() as usize,
        CHUNK_FILE_HEADER_LEN + CHUNK_TABLE_ENTRY_LEN
    );
}
