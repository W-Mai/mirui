use alloc::vec::Vec;
use core::fmt;
use core::iter::FusedIterator;
use core::slice;

use super::payload::{encode_error_for_image, resolve_node_payload};
use super::{ChunkNode, Document, DocumentState, PayloadStorage};
use crate::{ChunkFlags, ChunkId, ChunkType, EncodeError};

/// Logical provenance of a document payload.
///
/// Provenance is independent of whether the document borrows or owns its input
/// allocation. The representation is private so document internals remain
/// encapsulated.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub struct PayloadOrigin(u8);

impl PayloadOrigin {
    /// Payload retained from the input supplied when the document was opened.
    pub const ORIGINAL_SOURCE: Self = Self(0);
    /// Bytes borrowed independently of the document's input source.
    pub const BORROWED: Self = Self(1);
    /// Bytes owned by the document independently of its input source.
    pub const OWNED: Self = Self(2);
    /// Bytes produced from a structured payload representation.
    pub const SYNTHESIZED: Self = Self(3);
}

impl fmt::Debug for PayloadOrigin {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match *self {
            Self::ORIGINAL_SOURCE => "OriginalSource",
            Self::BORROWED => "Borrowed",
            Self::OWNED => "Owned",
            Self::SYNTHESIZED => "Synthesized",
            _ => "Unknown",
        };
        formatter.write_str(name)
    }
}

/// Immutable view of one chunk in an editable document.
#[derive(Clone, Copy)]
pub struct DocumentChunkRef<'a> {
    document: &'a Document<'a>,
    node: &'a ChunkNode<'a>,
}

impl<'a> DocumentChunkRef<'a> {
    pub const fn id(&self) -> ChunkId {
        self.node.id
    }

    pub const fn chunk_type(&self) -> ChunkType {
        self.node.chunk_type
    }

    pub const fn flags(&self) -> ChunkFlags {
        self.node.flags
    }

    pub const fn payload_origin(&self) -> PayloadOrigin {
        match &self.node.payload {
            PayloadStorage::SourceRange(_) => PayloadOrigin::ORIGINAL_SOURCE,
            PayloadStorage::Borrowed(_) => PayloadOrigin::BORROWED,
            PayloadStorage::Owned(_) => PayloadOrigin::OWNED,
            PayloadStorage::PromotedFlat => PayloadOrigin::SYNTHESIZED,
        }
    }

    /// Returns the encoded payload bytes when they already exist.
    pub fn payload_bytes(&self) -> Option<&'a [u8]> {
        resolve_node_payload(self.document, self.node).ok()?.bytes()
    }

    /// Returns the encoded payload length.
    ///
    /// The fallible result also supports payload representations whose encoded
    /// size must be planned before bytes are materialized.
    pub fn payload_len(&self) -> Result<usize, EncodeError> {
        resolve_node_payload(self.document, self.node)
            .map_err(encode_error_for_image)?
            .encoded_len()
    }

    /// Copies the complete encoded payload into caller-provided storage.
    ///
    /// The output remains unchanged when validation fails or when `out` is too
    /// short. Bytes after the returned payload length are untouched.
    pub fn copy_payload_into(&self, out: &mut [u8]) -> Result<usize, EncodeError> {
        resolve_node_payload(self.document, self.node)
            .map_err(encode_error_for_image)?
            .copy_into(out)
    }

    /// Materializes the complete encoded payload after computing its full size.
    pub fn payload_to_vec(&self) -> Result<Vec<u8>, EncodeError> {
        resolve_node_payload(self.document, self.node)
            .map_err(encode_error_for_image)?
            .to_vec()
    }
}

/// Lazy iterator over document chunks in table order.
#[derive(Clone)]
pub struct ChunkIter<'a> {
    document: &'a Document<'a>,
    nodes: slice::Iter<'a, ChunkNode<'a>>,
}

impl<'a> ChunkIter<'a> {
    fn new(document: &'a Document<'a>, nodes: &'a [ChunkNode<'a>]) -> Self {
        Self {
            document,
            nodes: nodes.iter(),
        }
    }

    fn view(&self, node: &'a ChunkNode<'a>) -> DocumentChunkRef<'a> {
        DocumentChunkRef {
            document: self.document,
            node,
        }
    }
}

impl<'a> Iterator for ChunkIter<'a> {
    type Item = DocumentChunkRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.nodes.next()?;
        Some(self.view(node))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }
}

impl DoubleEndedIterator for ChunkIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let node = self.nodes.next_back()?;
        Some(self.view(node))
    }
}

impl ExactSizeIterator for ChunkIter<'_> {}
impl FusedIterator for ChunkIter<'_> {}

/// Lazy iterator over chunks with one requested type.
#[derive(Clone)]
pub struct ChunksOfType<'a> {
    inner: ChunkIter<'a>,
    chunk_type: ChunkType,
}

impl<'a> Iterator for ChunksOfType<'a> {
    type Item = DocumentChunkRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .find(|chunk| chunk.chunk_type() == self.chunk_type)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.inner.len()))
    }
}

impl DoubleEndedIterator for ChunksOfType<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.inner
            .rfind(|chunk| chunk.chunk_type() == self.chunk_type)
    }
}

impl FusedIterator for ChunksOfType<'_> {}

impl<'source> Document<'source> {
    /// Iterates over all chunks in table order without allocating.
    pub fn chunks(&self) -> ChunkIter<'_> {
        let nodes = match &self.state {
            DocumentState::Chunk(chunks) => chunks.chunks.as_slice(),
            DocumentState::Flat(_) | DocumentState::OpaqueFlat(_) => &[],
        };
        ChunkIter::new(self, nodes)
    }

    /// Looks up a chunk by its stable document-session identity.
    pub fn get(&self, id: ChunkId) -> Option<DocumentChunkRef<'_>> {
        self.chunks().find(|chunk| chunk.id() == id)
    }

    /// Iterates over chunks of `chunk_type` in table order without allocating.
    pub fn chunks_of_type(&self, chunk_type: ChunkType) -> ChunksOfType<'_> {
        ChunksOfType {
            inner: self.chunks(),
            chunk_type,
        }
    }

    /// Returns the selected primary chunk identity, when present.
    pub const fn primary(&self) -> Option<ChunkId> {
        match &self.state {
            DocumentState::Chunk(chunks) => chunks.primary,
            DocumentState::Flat(_) | DocumentState::OpaqueFlat(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::header::{CHUNK_FILE_HEADER_LEN, CHUNK_TABLE_ENTRY_LEN, VERSION_MINOR, chunk_type};
    use crate::{ColorFormat, FlatImageInput, crc32, encode_chunks, encode_flat};

    fn set_primary(source: &mut [u8], chunk_type: u16) {
        source[20..22].copy_from_slice(&chunk_type.to_le_bytes());
        refresh_chunk_header_crc(source);
    }

    fn set_future_semantics(source: &mut [u8]) {
        source[5] = VERSION_MINOR + 1;
        refresh_chunk_header_crc(source);
    }

    fn refresh_chunk_header_crc(source: &mut [u8]) {
        let checksum = crc32(&source[..40]);
        source[40..44].copy_from_slice(&checksum.to_le_bytes());
    }

    fn table_payload_offset(source: &[u8], index: usize) -> usize {
        let entry = CHUNK_FILE_HEADER_LEN + index * CHUNK_TABLE_ENTRY_LEN;
        u32::from_le_bytes(source[entry + 4..entry + 8].try_into().unwrap()) as usize
    }

    fn ids<'a>(iter: impl Iterator<Item = DocumentChunkRef<'a>>) -> Vec<ChunkId> {
        iter.map(|chunk| chunk.id()).collect()
    }

    fn bytes_from_temporary<'a>(document: &'a Document<'_>, id: ChunkId) -> Option<&'a [u8]> {
        document.get(id)?.payload_bytes()
    }

    #[test]
    fn flat_opaque_flat_and_empty_chunk_documents_have_empty_queries() {
        let flat_source = encode_flat(&FlatImageInput {
            width: 1,
            height: 1,
            stride: 1,
            format: ColorFormat::A8,
            main: &[7],
            extra: None,
        });
        let flat = Document::open(&flat_source).unwrap();

        let mut opaque_source = flat_source.clone();
        opaque_source[5] = VERSION_MINOR + 1;
        let checksum = crc32(&opaque_source[..24]);
        opaque_source[24..28].copy_from_slice(&checksum.to_le_bytes());
        let opaque = Document::open(&opaque_source).unwrap();

        let new = Document::new();
        let id = ChunkId::new(0);
        for document in [&flat, &opaque, &new] {
            assert_eq!(document.chunks().len(), 0);
            assert_eq!(document.chunks().size_hint(), (0, Some(0)));
            assert!(document.get(id).is_none());
            assert!(document.chunks_of_type(ChunkType::META).next().is_none());
            assert_eq!(document.primary(), None);
        }
    }

    #[test]
    fn ordered_queries_preserve_duplicate_types_and_stable_ids() {
        let custom = ChunkType::new(0xbeef).unwrap();
        let mut source = encode_chunks(&[
            (custom.raw(), 0xa500, b"custom"),
            (chunk_type::FONT, 0x0100, b"first"),
            (chunk_type::META, 0, b"meta"),
            (chunk_type::FONT, 0x0200, b"second"),
        ]);
        set_primary(&mut source, chunk_type::FONT);
        let document = Document::open(&source).unwrap();

        let chunks: Vec<_> = document.chunks().collect();
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0].chunk_type(), custom);
        assert_eq!(chunks[0].flags().bits(), 0xa500);
        assert_eq!(chunks[0].payload_bytes(), Some(b"custom".as_slice()));
        assert_eq!(chunks[0].payload_len(), Ok(6));
        assert_eq!(chunks[0].payload_origin(), PayloadOrigin::ORIGINAL_SOURCE);

        let expected_ids: Vec<_> = (0..4).map(ChunkId::new).collect();
        assert_eq!(ids(document.chunks()), expected_ids);
        for (index, id) in expected_ids.iter().copied().enumerate() {
            assert_eq!(document.get(id).unwrap().id(), chunks[index].id());
        }
        assert!(document.get(ChunkId::new(100)).is_none());

        let fonts: Vec<_> = document.chunks_of_type(ChunkType::FONT).collect();
        assert_eq!(fonts.len(), 2);
        assert_eq!(fonts[0].id(), expected_ids[1]);
        assert_eq!(fonts[0].payload_bytes(), Some(b"first".as_slice()));
        assert_eq!(fonts[1].id(), expected_ids[3]);
        assert_eq!(fonts[1].payload_bytes(), Some(b"second".as_slice()));
        assert_eq!(document.primary(), Some(expected_ids[1]));
    }

    #[test]
    fn primary_query_handles_zero_stale_custom_duplicate_and_future_headers() {
        let custom = ChunkType::new(0xbeef).unwrap();
        let chunks = [
            (chunk_type::META, 0, b"meta".as_slice()),
            (custom.raw(), 0, b"first".as_slice()),
            (custom.raw(), 0, b"second".as_slice()),
        ];

        for primary_type in [0, chunk_type::VECTOR] {
            let mut source = encode_chunks(&chunks);
            set_primary(&mut source, primary_type);
            let document = Document::open(&source).unwrap();
            assert_eq!(document.primary(), None);
            assert_eq!(document.chunks().len(), 3);
        }

        let mut custom_source = encode_chunks(&chunks);
        set_primary(&mut custom_source, custom.raw());
        let custom_document = Document::open(&custom_source).unwrap();
        assert_eq!(custom_document.primary(), Some(ChunkId::new(1)));

        set_future_semantics(&mut custom_source);
        let future_document = Document::open(&custom_source).unwrap();
        assert!(future_document.file_metadata().has_future_semantics());
        assert_eq!(future_document.chunks().len(), 3);
        assert_eq!(future_document.primary(), Some(ChunkId::new(1)));
    }

    #[test]
    fn chunk_iterator_is_cloneable_exact_double_ended_and_fused() {
        fn assert_traits<T>()
        where
            T: Clone + DoubleEndedIterator + ExactSizeIterator + FusedIterator,
        {
        }

        assert_traits::<ChunkIter<'_>>();
        let source = encode_chunks(&[
            (chunk_type::META, 0, b"a"),
            (chunk_type::FONT, 0, b"b"),
            (chunk_type::VECTOR, 0, b"c"),
        ]);
        let document = Document::open(&source).unwrap();
        let mut iter = document.chunks();
        assert_eq!(iter.size_hint(), (3, Some(3)));
        assert_eq!(iter.len(), 3);
        assert_eq!(iter.next().unwrap().chunk_type(), ChunkType::META);
        assert_eq!(iter.len(), 2);

        let mut clone = iter.clone();
        assert_eq!(clone.next_back().unwrap().chunk_type(), ChunkType::VECTOR);
        assert_eq!(clone.len(), 1);
        assert_eq!(clone.next().unwrap().chunk_type(), ChunkType::FONT);
        assert!(clone.next().is_none());
        assert!(clone.next_back().is_none());
        assert!(clone.next().is_none());

        assert_eq!(iter.next_back().unwrap().chunk_type(), ChunkType::VECTOR);
        assert_eq!(iter.next().unwrap().chunk_type(), ChunkType::FONT);
        assert_eq!(iter.size_hint(), (0, Some(0)));
        assert!(iter.next().is_none());
        assert!(iter.next_back().is_none());
    }

    #[test]
    fn type_filter_is_cloneable_double_ended_fused_and_conservatively_bounded() {
        fn assert_traits<T>()
        where
            T: Clone + DoubleEndedIterator + FusedIterator,
        {
        }

        assert_traits::<ChunksOfType<'_>>();
        let source = encode_chunks(&[
            (chunk_type::FONT, 0, b"first"),
            (chunk_type::META, 0, b"meta"),
            (chunk_type::FONT, 0, b"middle"),
            (chunk_type::VECTOR, 0, b"vector"),
            (chunk_type::FONT, 0, b"last"),
        ]);
        let document = Document::open(&source).unwrap();
        let mut fonts = document.chunks_of_type(ChunkType::FONT);
        assert_eq!(fonts.size_hint(), (0, Some(5)));
        assert_eq!(
            fonts.next().unwrap().payload_bytes(),
            Some(b"first".as_slice())
        );
        assert_eq!(fonts.size_hint(), (0, Some(4)));

        let mut clone = fonts.clone();
        assert_eq!(
            clone.next_back().unwrap().payload_bytes(),
            Some(b"last".as_slice())
        );
        assert_eq!(clone.size_hint(), (0, Some(3)));
        assert_eq!(
            clone.next().unwrap().payload_bytes(),
            Some(b"middle".as_slice())
        );
        assert!(clone.next().is_none());
        assert!(clone.next_back().is_none());
        assert!(clone.next().is_none());

        assert_eq!(
            fonts.next_back().unwrap().payload_bytes(),
            Some(b"last".as_slice())
        );
        assert_eq!(
            fonts.next().unwrap().payload_bytes(),
            Some(b"middle".as_slice())
        );
        assert_eq!(fonts.size_hint(), (0, Some(1)));
        assert!(fonts.next().is_none());
        assert_eq!(fonts.size_hint(), (0, Some(0)));
    }

    #[test]
    fn source_payloads_keep_pointer_identity_overlap_empty_ranges_and_provenance() {
        let source = encode_chunks(&[
            (chunk_type::META, 0, b"abcd"),
            (chunk_type::FONT, 0, b"xy"),
            (chunk_type::PALETTE, 0, b""),
        ]);
        let borrowed_pointer = source[table_payload_offset(&source, 0)..].as_ptr();
        let borrowed = Document::open(&source).unwrap();
        let first_id = ChunkId::new(0);
        let empty_id = ChunkId::new(2);
        assert_eq!(
            bytes_from_temporary(&borrowed, first_id).unwrap().as_ptr(),
            borrowed_pointer
        );
        assert_eq!(borrowed.get(first_id).unwrap().payload_len(), Ok(4));
        assert_eq!(
            borrowed.get(first_id).unwrap().payload_origin(),
            PayloadOrigin::ORIGINAL_SOURCE
        );
        assert_eq!(
            borrowed.get(empty_id).unwrap().payload_bytes(),
            Some(&[][..])
        );
        assert_eq!(borrowed.get(empty_id).unwrap().payload_len(), Ok(0));

        let owned_source = source.clone();
        let owned_pointer = owned_source[table_payload_offset(&owned_source, 1)..].as_ptr();
        let owned = Document::from_vec(owned_source).unwrap();
        let second = owned.get(ChunkId::new(1)).unwrap();
        assert_eq!(second.payload_bytes().unwrap().as_ptr(), owned_pointer);
        assert_eq!(second.payload_origin(), PayloadOrigin::ORIGINAL_SOURCE);

        let mut overlap_source = source;
        let first_start = table_payload_offset(&overlap_source, 0);
        let second_entry = CHUNK_FILE_HEADER_LEN + CHUNK_TABLE_ENTRY_LEN;
        overlap_source[second_entry + 4..second_entry + 8]
            .copy_from_slice(&((first_start + 1) as u32).to_le_bytes());
        overlap_source[second_entry + 8..second_entry + 12].copy_from_slice(&2u32.to_le_bytes());
        let overlap = Document::open(&overlap_source).unwrap();
        assert_eq!(
            overlap.get(ChunkId::new(0)).unwrap().payload_bytes(),
            Some(b"abcd".as_slice())
        );
        assert_eq!(
            overlap.get(ChunkId::new(1)).unwrap().payload_bytes(),
            Some(b"bc".as_slice())
        );
    }

    #[test]
    fn public_query_handles_stay_within_embedded_size_budgets() {
        let word = core::mem::size_of::<usize>();
        assert_eq!(core::mem::size_of::<PayloadOrigin>(), 1);
        assert!(core::mem::size_of::<DocumentChunkRef<'_>>() <= 3 * word);
        assert!(core::mem::size_of::<ChunkIter<'_>>() <= 4 * word);
        assert!(core::mem::size_of::<ChunksOfType<'_>>() <= 5 * word);
    }
}
