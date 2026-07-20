mod source;

use alloc::vec::Vec;

use source::{Origin, SourceRange};

use crate::{DocumentError, Layout, ReadError, ReadOptions, Reader, VERSION_MAJOR, VERSION_MINOR};

const DEFAULT_MAX_CHUNKS: u16 = 256;

/// Immutable MIRX file metadata retained by an editable document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileMetaRef {
    version_major: u8,
    version_minor: u8,
    file_flags: u8,
}

impl FileMetaRef {
    pub const fn version_major(self) -> u8 {
        self.version_major
    }

    pub const fn version_minor(self) -> u8 {
        self.version_minor
    }

    pub const fn file_flags(self) -> u8 {
        self.file_flags
    }

    pub const fn has_future_semantics(self) -> bool {
        self.version_minor > VERSION_MINOR || self.file_flags != 0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileMeta {
    version_major: u8,
    version_minor: u8,
    file_flags: u8,
}

impl FileMeta {
    const CURRENT: Self = Self {
        version_major: VERSION_MAJOR,
        version_minor: VERSION_MINOR,
        file_flags: 0,
    };

    const fn as_ref(self) -> FileMetaRef {
        FileMetaRef {
            version_major: self.version_major,
            version_minor: self.version_minor,
            file_flags: self.file_flags,
        }
    }
}

/// Source layout retained until its editable records are materialized.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DocumentState {
    NewChunk,
    SourceFlat,
    OpaqueFlat,
    SourceChunk,
}

impl DocumentState {
    const fn layout(self) -> Layout {
        match self {
            Self::SourceFlat | Self::OpaqueFlat => Layout::Flat,
            Self::NewChunk | Self::SourceChunk => Layout::Chunk,
        }
    }
}

#[derive(Clone, Copy)]
struct OpenedDocument {
    logical_len: usize,
    file: FileMeta,
    state: DocumentState,
}

/// Source-backed editable MIRX document.
///
/// Opening from a slice borrows the exact input. Opening from a `Vec` moves the
/// allocation into the document. Neither path copies source bytes.
pub struct Document<'a> {
    origin: Origin<'a>,
    logical_len: usize,
    file: FileMeta,
    state: DocumentState,
    dirty: bool,
}

impl<'a> Document<'a> {
    /// Opens a document while borrowing its exact source bytes.
    pub fn open(source: &'a [u8]) -> Result<Self, DocumentError> {
        Self::from_origin(Origin::Borrowed(source))
    }

    /// Opens a document by moving its exact source allocation without copying.
    pub fn from_vec(source: Vec<u8>) -> Result<Self, DocumentError> {
        Self::from_origin(Origin::Owned(source))
    }

    /// Creates an empty MIRX 1.0 CHUNK document.
    pub const fn new_chunk() -> Self {
        Self {
            origin: Origin::New,
            logical_len: 0,
            file: FileMeta::CURRENT,
            state: DocumentState::NewChunk,
            dirty: true,
        }
    }

    pub const fn layout(&self) -> Layout {
        self.state.layout()
    }

    pub const fn file_meta(&self) -> FileMetaRef {
        self.file.as_ref()
    }

    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn from_origin(origin: Origin<'a>) -> Result<Self, DocumentError> {
        let opened = {
            let source = origin
                .source()
                .ok_or(DocumentError::Read(ReadError::SizeOverflow))?;
            inspect_source(source)?
        };
        let document = Self {
            origin,
            logical_len: opened.logical_len,
            file: opened.file,
            state: opened.state,
            dirty: false,
        };

        if !document.origin_contains_logical_source() {
            return Err(DocumentError::Read(ReadError::SizeOverflow));
        }
        Ok(document)
    }

    fn origin_contains_logical_source(&self) -> bool {
        let Some(source) = self.origin.source() else {
            return self.logical_len == 0;
        };
        let Some(range) = SourceRange::checked(0, self.logical_len, source.len()) else {
            return false;
        };
        self.origin.resolve(range).is_some()
    }
}

fn inspect_source(source: &[u8]) -> Result<OpenedDocument, DocumentError> {
    let options = ReadOptions::new().with_max_chunks(DEFAULT_MAX_CHUNKS);
    let reader = Reader::open_with(source, &options)?;
    let header = reader.file_header();
    let state = match (reader.layout(), reader.has_future_semantics()) {
        (Layout::Flat, true) => DocumentState::OpaqueFlat,
        (Layout::Flat, false) => DocumentState::SourceFlat,
        (Layout::Chunk, _) => DocumentState::SourceChunk,
    };
    Ok(OpenedDocument {
        logical_len: reader.logical_len(),
        file: FileMeta {
            version_major: header.version_major,
            version_minor: header.version_minor,
            file_flags: header.flags,
        },
        state,
    })
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::{
        ColorFormat, FlatImageInput, crc32, encode_chunks, encode_flat, header::chunk_type,
    };

    fn flat_source() -> Vec<u8> {
        encode_flat(&FlatImageInput {
            width: 2,
            height: 1,
            stride: 2,
            format: ColorFormat::A8,
            main: &[7, 9],
            extra: None,
        })
    }

    fn take_error(result: Result<Document<'_>, DocumentError>) -> DocumentError {
        match result {
            Ok(_) => panic!("expected document open failure"),
            Err(error) => error,
        }
    }

    #[test]
    fn borrowed_and_owned_open_retain_source_allocations() {
        let borrowed_source = flat_source();
        let borrowed_pointer = borrowed_source.as_ptr();
        let borrowed = Document::open(&borrowed_source).unwrap();
        assert_eq!(borrowed.origin.source().unwrap().as_ptr(), borrowed_pointer);

        let owned_source = flat_source();
        let owned_pointer = owned_source.as_ptr();
        let owned = Document::from_vec(owned_source).unwrap();
        assert_eq!(owned.origin.source().unwrap().as_ptr(), owned_pointer);

        assert_eq!(borrowed.layout(), owned.layout());
        assert_eq!(borrowed.state, DocumentState::SourceFlat);
        assert_eq!(owned.state, DocumentState::SourceFlat);
        assert_eq!(borrowed.file_meta(), owned.file_meta());
        assert_eq!(borrowed.logical_len, owned.logical_len);
        assert!(!borrowed.is_dirty());
        assert!(!owned.is_dirty());
    }

    #[test]
    fn owned_source_ranges_survive_document_moves() {
        fn move_document(document: Document<'_>) -> Document<'_> {
            document
        }

        let source = flat_source();
        let pointer = source.as_ptr();
        let document = Document::from_vec(source).unwrap();
        let range = SourceRange::checked(0, document.logical_len, document.logical_len).unwrap();
        let document = move_document(document);

        assert_eq!(document.origin.source().unwrap().as_ptr(), pointer);
        assert_eq!(document.origin.resolve(range).unwrap().as_ptr(), pointer);
    }

    #[test]
    fn borrowed_and_owned_fail_with_the_same_reader_error() {
        let malformed = vec![0; 8];
        let borrowed = take_error(Document::open(&malformed));
        let owned = take_error(Document::from_vec(malformed));

        assert_eq!(borrowed, DocumentError::Read(ReadError::BadMagic));
        assert_eq!(owned, borrowed);
    }

    #[test]
    fn new_chunk_has_current_editable_dirty_state_without_a_source() {
        let document = Document::new_chunk();

        assert_eq!(document.layout(), Layout::Chunk);
        assert_eq!(document.state, DocumentState::NewChunk);
        assert_eq!(
            document.file_meta(),
            FileMetaRef {
                version_major: VERSION_MAJOR,
                version_minor: VERSION_MINOR,
                file_flags: 0,
            }
        );
        assert!(document.is_dirty());
        assert_eq!(document.logical_len, 0);
        assert!(document.origin.source().is_none());
        assert!(document.origin_contains_logical_source());
    }

    #[test]
    fn future_flat_sources_are_retained_as_opaque_state() {
        for (minor, flags) in [(VERSION_MINOR + 1, 0), (VERSION_MINOR, 0x80)] {
            let mut source = flat_source();
            source[5] = minor;
            source[7] = flags;
            let checksum = crc32(&source[..24]);
            source[24..28].copy_from_slice(&checksum.to_le_bytes());

            let document = Document::open(&source).unwrap();
            assert_eq!(document.layout(), Layout::Flat);
            assert_eq!(document.state, DocumentState::OpaqueFlat);
            assert!(!document.is_dirty());
            assert_eq!(document.file_meta().version_minor(), minor);
            assert_eq!(document.file_meta().file_flags(), flags);
            assert!(document.file_meta().has_future_semantics());
            assert_eq!(document.origin.source().unwrap().as_ptr(), source.as_ptr());
        }
    }

    #[test]
    fn future_chunk_sources_keep_their_source_state_and_metadata() {
        for (minor, flags) in [(VERSION_MINOR + 1, 0), (VERSION_MINOR, 0x80)] {
            let mut source = encode_chunks(&[]);
            source[5] = minor;
            source[7] = flags;
            let checksum = crc32(&source[..40]);
            source[40..44].copy_from_slice(&checksum.to_le_bytes());

            let document = Document::open(&source).unwrap();
            assert_eq!(document.layout(), Layout::Chunk);
            assert_eq!(document.state, DocumentState::SourceChunk);
            assert!(!document.is_dirty());
            assert_eq!(document.file_meta().version_minor(), minor);
            assert_eq!(document.file_meta().file_flags(), flags);
            assert!(document.file_meta().has_future_semantics());
            assert_eq!(document.origin.source().unwrap().as_ptr(), source.as_ptr());
        }
    }

    #[test]
    fn document_default_accepts_256_chunks_and_rejects_257() {
        let chunks = vec![(chunk_type::META, 0, &[][..]); 256];
        let source = encode_chunks(&chunks);
        assert!(Document::open(&source).is_ok());

        let chunks = vec![(chunk_type::META, 0, &[][..]); 257];
        let source = encode_chunks(&chunks);
        assert_eq!(
            take_error(Document::open(&source)),
            DocumentError::Read(ReadError::TooManyChunks {
                count: 257,
                limit: DEFAULT_MAX_CHUNKS,
            })
        );
    }

    #[test]
    fn opened_chunk_stays_source_backed_until_nodes_are_materialized() {
        let source = encode_chunks(&[]);
        let document = Document::open(&source).unwrap();

        assert_eq!(document.layout(), Layout::Chunk);
        assert_eq!(document.state, DocumentState::SourceChunk);
        assert!(!document.is_dirty());
        assert_eq!(document.origin.source().unwrap().as_ptr(), source.as_ptr());
    }

    #[test]
    fn document_open_uses_exact_length_policy() {
        let mut source = encode_chunks(&[]);
        let logical_len = source.len();
        source.extend_from_slice(&[1, 2, 3]);

        assert_eq!(
            take_error(Document::open(&source)),
            DocumentError::Read(ReadError::TrailingBytes {
                logical_len,
                actual_len: source.len(),
            })
        );
    }
}
