use super::{ContainerHeader, ReadError, Reader};
use crate::{ChunkRef, ChunkType, PrimaryHints};

impl<'a> Reader<'a> {
    /// Resolves the first table entry matching the wire primary type.
    pub fn primary(&self) -> Result<Option<ChunkRef<'a>>, ReadError> {
        let ContainerHeader::Chunk(header) = self.header else {
            return Ok(None);
        };
        let Some(chunk_type) = ChunkType::new(header.primary_chunk_type) else {
            return Ok(None);
        };
        Ok(self.chunks().find(|chunk| chunk.chunk_type() == chunk_type))
    }

    /// Returns the raw primary display hints without normalizing legacy or
    /// future values.
    pub const fn primary_hints(&self) -> PrimaryHints {
        match self.header {
            ContainerHeader::Flat(header) => PrimaryHints::new(
                crate::image::SampleLayout::new(header.color_format as u16),
                header.width,
                header.height,
                header.stride,
            ),
            ContainerHeader::Chunk(header) => PrimaryHints::new(
                crate::image::SampleLayout::new(header.primary_sample_layout),
                header.primary_width,
                header.primary_height,
                header.primary_stride,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::header::{CHUNK_FILE_HEADER_LEN, VERSION_MINOR, chunk_type};
    use crate::{ColorFormat, Layout, crc32, encode_chunks};

    fn set_primary(
        bytes: &mut [u8],
        chunk_type: u16,
        sample_layout: u16,
        width: u32,
        height: u32,
        stride: u32,
    ) {
        bytes[20..22].copy_from_slice(&chunk_type.to_le_bytes());
        bytes[22..24].copy_from_slice(&sample_layout.to_le_bytes());
        bytes[24..28].copy_from_slice(&width.to_le_bytes());
        bytes[28..32].copy_from_slice(&height.to_le_bytes());
        bytes[32..36].copy_from_slice(&stride.to_le_bytes());
        let checksum = crc32(&bytes[..40]);
        bytes[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    fn flat_a8() -> alloc::vec::Vec<u8> {
        let mut bytes = vec![0; crate::FLAT_HEADER_LEN + 1];
        bytes[..4].copy_from_slice(b"MIRX");
        bytes[4] = crate::VERSION_MAJOR;
        bytes[5] = VERSION_MINOR;
        bytes[6] = Layout::Flat.to_u8();
        bytes[8] = ColorFormat::A8.to_u8();
        bytes[12..16].copy_from_slice(&1u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        let checksum = crc32(&bytes[..24]);
        bytes[24..28].copy_from_slice(&checksum.to_le_bytes());
        bytes
    }

    #[test]
    fn flat_has_image_hints_but_no_chunk_primary() {
        let bytes = flat_a8();
        let reader = Reader::open(&bytes).unwrap();
        assert_eq!(reader.primary(), Ok(None));
        assert_eq!(
            reader.primary_hints(),
            PrimaryHints::new(
                crate::image::SampleLayout::from_color_format(ColorFormat::A8),
                1,
                1,
                1
            )
        );
    }

    #[test]
    fn primary_type_zero_preserves_raw_hints() {
        let mut bytes = encode_chunks(&[]);
        set_primary(&mut bytes, 0, 0xfedc, 13, 21, 55);
        let reader = Reader::open(&bytes).unwrap();
        assert_eq!(reader.primary(), Ok(None));
        assert_eq!(
            reader.primary_hints(),
            PrimaryHints::new(crate::image::SampleLayout::new(0xfedc), 13, 21, 55)
        );
        assert_eq!(reader.primary_hints().known_color_format(), None);
    }

    #[test]
    fn primary_layout_uses_both_header_bytes() {
        for layout in [
            crate::image::SampleLayout::I420,
            crate::image::SampleLayout::NV12,
            crate::image::SampleLayout::P010,
            crate::image::SampleLayout::new(0xfedc),
        ] {
            let mut bytes = encode_chunks(&[(0xbeef, 0, b"opaque")]);
            set_primary(&mut bytes, 0xbeef, layout.raw(), 320, 240, 640);
            let reader = Reader::open(&bytes).unwrap();
            assert_eq!(reader.primary_hints().sample_layout(), layout);
            assert_eq!(reader.primary_hints().known_color_format(), None);
        }
    }

    #[test]
    fn resolves_the_first_matching_entry_in_table_order() {
        let mut bytes = encode_chunks(&[
            (chunk_type::META, 0, b"meta"),
            (chunk_type::FONT, 0, b"first"),
            (chunk_type::FONT, 0, b"second"),
        ]);
        set_primary(
            &mut bytes,
            chunk_type::FONT,
            crate::image::SampleLayout::NONE.raw(),
            0,
            0,
            0,
        );
        let reader = Reader::open(&bytes).unwrap();
        let primary = reader.primary().unwrap().unwrap();
        assert_eq!(primary.index(), 1);
        assert_eq!(primary.payload(), b"first");
        assert_eq!(primary.chunk_type(), ChunkType::FONT);
    }

    #[test]
    fn resolves_custom_types_and_keeps_legacy_zero_hints() {
        let mut bytes = encode_chunks(&[(0xbeef, 0, b"custom")]);
        set_primary(&mut bytes, 0xbeef, 0, 0, 0, 0);
        let reader = Reader::open(&bytes).unwrap();
        let primary = reader.primary().unwrap().unwrap();
        assert_eq!(primary.chunk_type().raw(), 0xbeef);
        assert_eq!(primary.payload(), b"custom");
        assert_eq!(reader.primary_hints(), PrimaryHints::ZERO);
    }

    #[test]
    fn stale_type_remains_queryable() {
        let mut bytes = encode_chunks(&[(chunk_type::META, 0, b"meta")]);
        set_primary(&mut bytes, chunk_type::VECTOR, 0xa5, 3, 4, 12);
        let reader = Reader::open(&bytes).unwrap();
        assert_eq!(reader.primary(), Ok(None));
        assert_eq!(
            reader.primary_hints(),
            PrimaryHints::new(crate::image::SampleLayout::new(0xa5), 3, 4, 12)
        );
        assert_eq!(reader.chunks().next().unwrap().payload(), b"meta");
    }

    #[test]
    fn matching_primary_payload_is_bound_to_the_reader_source() {
        let mut bytes = encode_chunks(&[(chunk_type::IMAGE, 0, b"image")]);
        set_primary(
            &mut bytes,
            chunk_type::IMAGE,
            crate::image::SampleLayout::RGBA8888.raw(),
            2,
            3,
            8,
        );
        let reader = Reader::open(&bytes).unwrap();
        let primary = reader.primary().unwrap().unwrap();
        assert_eq!(
            primary.payload().as_ptr(),
            bytes[primary.payload_offset() as usize..].as_ptr()
        );
        assert_eq!(
            reader.primary_hints().known_color_format(),
            Some(ColorFormat::RGBA8888)
        );
        assert!(primary.payload_offset() as usize >= CHUNK_FILE_HEADER_LEN);
    }
}
